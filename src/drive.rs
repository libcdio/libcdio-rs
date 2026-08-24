// Copyright (C) 2026 Shiva Kiran Koninty <shiva@skran.xyz>
//
// This file is part of libcdio-rs.
//
// libcdio-rs is free software: you can redistribute it and/or
// modify it under the terms of the GNU General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// libcdio-rs is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with libcdio-rs. If not, see <https://www.gnu.org/licenses/>.

//! Routines related to CD/DVD drives.

use std::{
    error::Error,
    ffi::{CStr, CString, OsString},
    fmt,
    mem::MaybeUninit,
    path::{Path, PathBuf},
};

use bitflags::bitflags;
use libcdio_sys::cdio_hwinfo_t;
use thiserror::Error;

use crate::{
    cdio::{CDIO_INIT_LOCK, Cdio},
    logging,
};

/// An interface to a disc drive.
pub struct Drive {
    pub(crate) cdio: Cdio,
}

impl Drive {
    /// Returns a list of connected drives.
    pub fn drives() -> Vec<PathBuf> {
        logging::init_logger();

        // SAFETY: This method internally initializes an instance of CdIo_t,
        // which is not thread safe. Hold CDIO_INIT_LOCK to uphold thread
        // safety.
        let _lock = CDIO_INIT_LOCK.lock().unwrap();
        let drive_list =
            unsafe { libcdio_sys::cdio_get_devices(libcdio_sys::driver_id_t_DRIVER_DEVICE) };
        if drive_list.is_null() {
            return vec![];
        }

        let mut drives = Vec::new();
        let mut ptr = drive_list;

        // SAFETY: Null checked
        while !ptr.is_null()
            && let drive = unsafe { *ptr }
            && !drive.is_null()
        {
            // SAFETY: `drive` represents a system path, making it a valid `OsString`
            drives.push(PathBuf::from(unsafe {
                OsString::from_encoded_bytes_unchecked(CStr::from_ptr(drive).to_bytes().to_vec())
            }));
            ptr = unsafe { ptr.offset(1) };
        }

        // SAFETY: drive_list has been copied into drives
        unsafe {
            libcdio_sys::cdio_free_device_list(drive_list);
        }

        drives
    }

    /// Opens a default connected drive.
    pub fn new() -> Result<Self, DriveNotFoundError> {
        Cdio::with_device(None)
            .ok_or(DriveNotFoundError)
            .map(|cdio| Self { cdio })
    }

    /// Opens drive at given path.
    ///
    /// See [`Self::drives()`] for a list of connected drives.
    pub fn with_drive(drive: PathBuf) -> Result<Self, DriveOpenError> {
        let drive = CString::new(drive.into_os_string().into_encoded_bytes())
            .map_err(|err| DriveOpenError::new(err.clone().into_vec(), err.into()))?;
        let cdio = Cdio::with_device(Some(&drive)).ok_or_else(|| {
            DriveOpenError::new(drive.into_bytes(), "cdio_open_am() returned NULL".into())
        })?;

        Ok(Self { cdio })
    }

    /// Returns hardware identifiers of the drive such as Model, Vendor and Revision.
    pub fn hardware_identifiers(&self) -> Result<HardwareIdentifiers, DriveOperationError> {
        let mut hwinfo: MaybeUninit<cdio_hwinfo_t> = MaybeUninit::uninit();
        let ret = unsafe { libcdio_sys::cdio_get_hwinfo(self.cdio.as_ptr(), hwinfo.as_mut_ptr()) };
        if !ret {
            return Err(DriveOperationError);
        }

        // SAFETY: cdio_get_hwinfo() returned true, therefore hwinfo should be initialized
        let hwinfo = unsafe { hwinfo.assume_init() };

        // SAFETY: The strings are null terminated
        unsafe {
            let model = CStr::from_ptr(hwinfo.psz_model.as_ptr());
            let vendor = CStr::from_ptr(hwinfo.psz_vendor.as_ptr());
            let revision = CStr::from_ptr(hwinfo.psz_revision.as_ptr());

            Ok(HardwareIdentifiers {
                model: model.to_string_lossy().trim_end().to_string(),
                vendor: vendor.to_string_lossy().trim_end().to_string(),
                revision: revision.to_string_lossy().trim_end().to_string(),
            })
        }
    }

    /// Returns drive capabilities.
    pub fn capabilities(&self) -> Result<DriveCapabilities, DriveOperationError> {
        let mut read = 0;
        let mut write = 0;
        let mut misc = 0;
        unsafe {
            libcdio_sys::cdio_get_drive_cap(self.cdio.as_ptr(), &mut read, &mut write, &mut misc);
        }

        (|| {
            Some(DriveCapabilities {
                read: ReadCapabilities::from_bits(read)?,
                write: WriteCapabilities::from_bits(write)?,
                misc: MiscCapabilities::from_bits(misc)?,
            })
        })()
        .ok_or(DriveOperationError)
    }
}

#[non_exhaustive]
#[derive(Debug, Error)]
#[error("could not find any drives")]
pub struct DriveNotFoundError;

#[derive(Debug, Error)]
#[error(transparent)]
pub struct DriveOpenError(Box<OpenErrRepr>);

#[derive(Debug, Error)]
#[error("could not open drive at `{path}`")]
struct OpenErrRepr {
    path: PathBuf,
    source: Box<dyn Error + Send + Sync>,
}

impl DriveOpenError {
    /// Returns the system path of the drive.
    pub fn path(&self) -> &Path {
        &self.0.path
    }

    fn new(path_bytes: Vec<u8>, source: Box<dyn Error + Send + Sync>) -> Self {
        Self(Box::new(OpenErrRepr {
            // SAFETY: path_bytes originate from a PathBuf
            path: unsafe { OsString::from_encoded_bytes_unchecked(path_bytes) }.into(),
            source,
        }))
    }
}

#[non_exhaustive]
#[derive(Debug, Error)]
#[error("could not perform operation on the drive")]
pub struct DriveOperationError;

/// Hardware identifiers such as model, vendor and revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HardwareIdentifiers {
    pub model: String,
    pub vendor: String,
    pub revision: String,
}

/// Drive capabilities.
#[derive(Clone, Copy, Debug)]
pub struct DriveCapabilities {
    pub read: ReadCapabilities,
    pub write: WriteCapabilities,
    pub misc: MiscCapabilities,
}
// the C enum discriminants are explicit, positive and fit a u32, making these casts safe
bitflags! {
    /// Miscellaneous capabilities of the drive.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct MiscCapabilities: u32 {
        /// Can close tray
        const CloseTray = libcdio_sys::cdio_drive_cap_misc_t_CDIO_DRIVE_CAP_MISC_CLOSE_TRAY as _;
        /// Can eject
        const Eject = libcdio_sys::cdio_drive_cap_misc_t_CDIO_DRIVE_CAP_MISC_EJECT as _;
        /// Can disable manual eject
        const Lock = libcdio_sys::cdio_drive_cap_misc_t_CDIO_DRIVE_CAP_MISC_LOCK as _;
        /// Can set drive speed
        const SelectSpeed = libcdio_sys::cdio_drive_cap_misc_t_CDIO_DRIVE_CAP_MISC_SELECT_SPEED as _;
        /// Can select juke-box disc
        const SelectDisc = libcdio_sys::cdio_drive_cap_misc_t_CDIO_DRIVE_CAP_MISC_SELECT_DISC as _;
        /// Can read multiple sessions
        const MultiSession = libcdio_sys::cdio_drive_cap_misc_t_CDIO_DRIVE_CAP_MISC_MULTI_SESSION as _;
        /// Can detect if media changed
        const MediaChanged = libcdio_sys::cdio_drive_cap_misc_t_CDIO_DRIVE_CAP_MISC_MEDIA_CHANGED as _;
        /// Can hard reset device
        const Reset = libcdio_sys::cdio_drive_cap_misc_t_CDIO_DRIVE_CAP_MISC_RESET as _;
    }
}
// the C enum discriminants are explicit, positive and fit a u32, making these casts safe
bitflags! {
    /// Read capabilities of the drive.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct ReadCapabilities: u32 {
        /// Can play audio
        const Audio = libcdio_sys::cdio_drive_cap_read_t_CDIO_DRIVE_CAP_READ_AUDIO as _;
        /// Can read CD-DA
        const CdDa = libcdio_sys::cdio_drive_cap_read_t_CDIO_DRIVE_CAP_READ_CD_DA as _;
        /// Can read CD+G
        const CdPlusG = libcdio_sys::cdio_drive_cap_read_t_CDIO_DRIVE_CAP_READ_CD_G as _;
        /// Can read CD-R
        const CdR = libcdio_sys::cdio_drive_cap_read_t_CDIO_DRIVE_CAP_READ_CD_R as _;
        /// Can read CD-RW
        const CdRw = libcdio_sys::cdio_drive_cap_read_t_CDIO_DRIVE_CAP_READ_CD_RW as _;
        /// Can read DVD-R
        const DvdR = libcdio_sys::cdio_drive_cap_read_t_CDIO_DRIVE_CAP_READ_DVD_R as _;
        /// Can read DVD+R
        const DvdPlusR = libcdio_sys::cdio_drive_cap_read_t_CDIO_DRIVE_CAP_READ_DVD_PR as _;
        /// Can read DVD-RAM
        const DvdRam = libcdio_sys::cdio_drive_cap_read_t_CDIO_DRIVE_CAP_READ_DVD_RAM as _;
        /// Can read DVD-ROM
        const DvdRom = libcdio_sys::cdio_drive_cap_read_t_CDIO_DRIVE_CAP_READ_DVD_ROM as _;
        /// Can read DVD-RW
        const DvdRw = libcdio_sys::cdio_drive_cap_read_t_CDIO_DRIVE_CAP_READ_DVD_RW as _;
        /// Can read DVD+RW
        const DvdPlusRw = libcdio_sys::cdio_drive_cap_read_t_CDIO_DRIVE_CAP_READ_DVD_RPW as _;
        /// Can read C2 errors
        const C2Errors = libcdio_sys::cdio_drive_cap_read_t_CDIO_DRIVE_CAP_READ_C2_ERRS as _;
        /// Can read Mode 2 Form 1 (VCD)
        const Mode2Form1 = libcdio_sys::cdio_drive_cap_read_t_CDIO_DRIVE_CAP_READ_MODE2_FORM1 as _;
        /// Can read Mode 2 Form 2 (VCD)
        const Mode2Form2 = libcdio_sys::cdio_drive_cap_read_t_CDIO_DRIVE_CAP_READ_MODE2_FORM2 as _;
        /// Can read MCN
        const Mcn = libcdio_sys::cdio_drive_cap_read_t_CDIO_DRIVE_CAP_READ_MCN as _;
        /// Can read ISRC
        const Isrc = libcdio_sys::cdio_drive_cap_read_t_CDIO_DRIVE_CAP_READ_ISRC as _;
    }
}
// the C enum discriminants are explicit, positive and fit a u32, making these casts safe
bitflags! {
    /// Write capabilities of the drive.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct WriteCapabilities: u32 {
        /// Can write CD-R
        const CdR = libcdio_sys::cdio_drive_cap_write_t_CDIO_DRIVE_CAP_WRITE_CD_R as _;
        /// Can write CD-RW
        const CdRw = libcdio_sys::cdio_drive_cap_write_t_CDIO_DRIVE_CAP_WRITE_CD_RW as _;
        /// Can write DVD-R
        const DvdR = libcdio_sys::cdio_drive_cap_write_t_CDIO_DRIVE_CAP_WRITE_DVD_R as _;
        /// Can write DVD+R
        const DvdPlusR = libcdio_sys::cdio_drive_cap_write_t_CDIO_DRIVE_CAP_WRITE_DVD_PR as _;
        /// Can write DVD-RAM
        const DvdRam = libcdio_sys::cdio_drive_cap_write_t_CDIO_DRIVE_CAP_WRITE_DVD_RAM as _;
        /// Can write DVD-RW
        const DvdRw = libcdio_sys::cdio_drive_cap_write_t_CDIO_DRIVE_CAP_WRITE_DVD_RW as _;
        /// Can write DVD+RW
        const DvdPlusRw = libcdio_sys::cdio_drive_cap_write_t_CDIO_DRIVE_CAP_WRITE_DVD_RPW as _;
        /// Can write MRW (Mount Rainier)
        const Mrw = libcdio_sys::cdio_drive_cap_write_t_CDIO_DRIVE_CAP_WRITE_MT_RAINIER as _;
        /// Can write using Burn proof
        const BurnProof = libcdio_sys::cdio_drive_cap_write_t_CDIO_DRIVE_CAP_WRITE_BURN_PROOF as _;
    }
}

impl fmt::Display for MiscCapabilities {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            MiscCapabilities::CloseTray => write!(f, "Close Tray"),
            MiscCapabilities::Eject => write!(f, "Eject"),
            MiscCapabilities::Lock => write!(f, "Lock"),
            MiscCapabilities::SelectSpeed => write!(f, "Select Speed"),
            MiscCapabilities::SelectDisc => write!(f, "Select Disc"),
            MiscCapabilities::MultiSession => write!(f, "Multi Session"),
            MiscCapabilities::MediaChanged => write!(f, "Media Change Detection"),
            MiscCapabilities::Reset => write!(f, "Hard Reset"),
            _ => write!(f, "Unknown"),
        }
    }
}
impl fmt::Display for ReadCapabilities {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            ReadCapabilities::Audio => write!(f, "Play Audio"),
            ReadCapabilities::CdDa => write!(f, "CD-DA"),
            ReadCapabilities::CdPlusG => write!(f, "CD+G"),
            ReadCapabilities::CdR => write!(f, "CD-R"),
            ReadCapabilities::CdRw => write!(f, "CD-RW"),
            ReadCapabilities::DvdR => write!(f, "DVD-R"),
            ReadCapabilities::DvdPlusR => write!(f, "DVD+R"),
            ReadCapabilities::DvdRam => write!(f, "DVD-RAM"),
            ReadCapabilities::DvdRom => write!(f, "DVD-ROM"),
            ReadCapabilities::DvdRw => write!(f, "DVD-RW"),
            ReadCapabilities::DvdPlusRw => write!(f, "DVD+RW"),
            ReadCapabilities::C2Errors => write!(f, "C2 Errors"),
            ReadCapabilities::Mode2Form1 => write!(f, "Mode 2 Form 1 (VCD)"),
            ReadCapabilities::Mode2Form2 => write!(f, "Mode 2 Form 2 (VCD)"),
            ReadCapabilities::Mcn => write!(f, "MCN"),
            ReadCapabilities::Isrc => write!(f, "ISRC"),
            _ => write!(f, "Unknown"),
        }
    }
}
impl fmt::Display for WriteCapabilities {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            WriteCapabilities::CdR => write!(f, "CD-R"),
            WriteCapabilities::CdRw => write!(f, "CD-RW"),
            WriteCapabilities::DvdR => write!(f, "DVD-R"),
            WriteCapabilities::DvdPlusR => write!(f, "DVD+R"),
            WriteCapabilities::DvdRam => write!(f, "DVD-RAM"),
            WriteCapabilities::DvdRw => write!(f, "DVD-RW"),
            WriteCapabilities::DvdPlusRw => write!(f, "DVD+RW"),
            WriteCapabilities::Mrw => write!(f, "MRW"),
            WriteCapabilities::BurnProof => write!(f, "Burn Proof"),
            _ => write!(f, "Unknown"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires a disc drive"]
    fn with_drive() {
        Drive::with_drive(PathBuf::from("/dev/cdrom")).unwrap();
    }

    #[test]
    #[ignore = "requires a disc drive"]
    fn drives() {
        assert!(!Drive::drives().is_empty());
    }

    #[test]
    #[ignore = "requires a disc drive"]
    fn hardware_identifiers() {
        Drive::new().unwrap().hardware_identifiers().unwrap();
    }

    #[test]
    #[ignore = "requires a disc drive"]
    fn capabilities() {
        Drive::new().unwrap().capabilities().unwrap();
    }
}
