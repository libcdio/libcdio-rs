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

//! SCSI MMC (MultiMedia Commands) routines.
//! Refer to `README.md` for the reference manuals of SPC and MMC used.

use std::{
    ffi::{CString, NulError, OsString},
    path::PathBuf,
    ptr,
};

pub use get_config::*;
pub use get_event_status::*;
pub use inquiry::*;
pub use prevent_allow_medium_removal::*;
pub use read_disc_info::*;
pub use read_subchannel::*;
pub use read_toc::*;
pub use set_cd_speed::*;
pub use start_stop_unit::*;
pub use test_unit_ready::*;

mod get_config;
mod get_event_status;
mod inquiry;
mod prevent_allow_medium_removal;
mod read_disc_info;
mod read_subchannel;
mod read_toc;
mod set_cd_speed;
mod start_stop_unit;
mod test_unit_ready;

use docsplay::Display;
use libcdio_sys::{
    cdio_mmc_level_t_CDIO_MMC_LEVEL_1, cdio_mmc_level_t_CDIO_MMC_LEVEL_2,
    cdio_mmc_level_t_CDIO_MMC_LEVEL_3, cdio_mmc_level_t_CDIO_MMC_LEVEL_NONE,
    cdio_mmc_level_t_CDIO_MMC_LEVEL_WEIRD,
};
use num_enum::{FromPrimitive, TryFromPrimitive};
use thiserror::Error;

use crate::cdio::Cdio;

/// An interface for SCSI MMC commands.
pub struct Mmc {
    cdio: Cdio,
}

/// Represents the MMC Level.
#[non_exhaustive]
#[repr(u32)]
#[derive(
    Clone, Debug, Default, Display, Eq, Hash, Ord, PartialEq, PartialOrd, TryFromPrimitive,
)]
pub enum MmcLevel {
    #[default]
    /// Unknown
    Unknown = cdio_mmc_level_t_CDIO_MMC_LEVEL_WEIRD,
    /// MMC-1
    Mmc1 = cdio_mmc_level_t_CDIO_MMC_LEVEL_1,
    /// MMC-2
    Mmc2 = cdio_mmc_level_t_CDIO_MMC_LEVEL_2,
    /// MMC-3
    Mmc3 = cdio_mmc_level_t_CDIO_MMC_LEVEL_3,
}

impl Mmc {
    /// Use a default device.
    ///
    /// # Errors
    /// If an MMC capable device could not be found.
    pub fn new() -> Result<Mmc, MmcNotFoundError> {
        Cdio::with_device(None)
            .map(|cdio| Self { cdio })
            .filter(|mmc| mmc.level().is_ok())
            .ok_or(MmcNotFoundError)
    }

    /// Use the provided device.
    ///
    /// # Errors
    /// If there are no devices with MMC connected, or the device could not be
    /// opened.
    pub fn with_device(device: PathBuf) -> Result<Mmc, WithDeviceError> {
        let device = CString::new(device.into_os_string().into_encoded_bytes()).map_err(|err| {
            WithDeviceError {
                device: os_string_from_bytes_safe(err.clone().into_vec()).into(),
                source: WithDeviceErrorKind::DeviceHasNullChar(err),
            }
        })?;
        let Some(cdio) = Cdio::with_device(Some(&device)) else {
            return Err(WithDeviceError {
                device: os_string_from_bytes_safe(device.into_bytes()).into(),
                source: WithDeviceErrorKind::CouldNotOpenDevice,
            });
        };
        let mmc = Self { cdio };
        return mmc.level().map(|_| mmc).map_err(|_| WithDeviceError {
            device: os_string_from_bytes_safe(device.into_bytes()).into(),
            source: WithDeviceErrorKind::MmcNotSupported,
        });

        fn os_string_from_bytes_safe(bytes: Vec<u8>) -> OsString {
            // SAFETY: the bytes originate from an OsString
            unsafe { OsString::from_encoded_bytes_unchecked(bytes) }
        }
    }

    /// Get the MMC level supported by the drive.
    ///
    /// # Errors
    /// If an underlying operation failed, or if the device is unavailable.
    pub fn level(&self) -> Result<MmcLevel, MmcOperationError> {
        let mmc_level = unsafe { libcdio_sys::mmc_get_drive_mmc_cap(self.cdio.as_ptr()) };
        if mmc_level == cdio_mmc_level_t_CDIO_MMC_LEVEL_NONE {
            return Err(MmcOperationError);
        }

        Ok(MmcLevel::try_from(mmc_level)
            .expect("mmc_get_drive_mmc_cap should return a valid mmc_level_t"))
    }

    /// Returns the current sense data from the device.
    pub fn sense_data(&self) -> Option<MmcSenseData> {
        let mut sense_ptr = ptr::null_mut();
        let ret = unsafe { libcdio_sys::mmc_last_cmd_sense(self.cdio.as_ptr(), &mut sense_ptr) };
        if ret <= 0 || sense_ptr.is_null() {
            return None;
        }
        // SAFETY: Null check done.
        let sense = unsafe { *sense_ptr };
        let sense = MmcSenseData {
            sense_key: SenseKey::from(sense.sense_key()),
            asc: sense.asc,
            ascq: sense.ascq,
            ili: sense.ili() != 0,
            csi: sense.command_info,
            fruc: sense.fruc,
            sks: sense.sks,
            asb: sense.asb,
        };
        // SAFETY: The contents have been copied.
        unsafe { libcdio_sys::cdio_free(sense_ptr.cast()) };

        Some(sense)
    }

    fn run_command(
        &self,
        direction: Option<MmcDirection>,
        buf: &mut [u8],
        cdb: Cdb,
    ) -> Result<(), MmcError> {
        // the cast is safe as MmcDirection's discriminants are
        // small (< i8::MAX) and non-negative
        let direction = direction
            .map(|d| d as _)
            .unwrap_or(libcdio_sys::mmc_direction_s_SCSI_MMC_DATA_NONE);
        let cdb = libcdio_sys::mmc_cdb_s { field: cdb };
        let ret = unsafe {
            libcdio_sys::mmc_run_cmd(
                self.cdio.as_ptr(),
                DEFAULT_TIMEOUT_MS,
                &cdb,
                direction,
                buf.len()
                    .try_into()
                    .expect("failed to cast length of buf passed to Mmc::run_command()"),
                buf.as_mut_ptr().cast(),
            )
        };
        return if ret >= 0 {
            Ok(())
        } else if ret == -1
            && let Some(sense_data) = self.sense_data()
        {
            Err(MmcError::CheckCondition(sense_data))
        } else {
            Err(MmcError::Os(OsError::from(ret)))
        };

        const DEFAULT_TIMEOUT_MS: u32 = 6000;
    }
}
type Cdb = [u8; 12];

/// error opening MMC device at `{device}`
#[derive(Debug, Display, Error)]
pub struct WithDeviceError {
    pub device: PathBuf,
    pub source: WithDeviceErrorKind,
}
/// Error kind of [`WithDeviceError`]
#[derive(Debug, Display, Error)]
pub enum WithDeviceErrorKind {
    /// device path contains null character
    DeviceHasNullChar(NulError),
    /// could not open device
    CouldNotOpenDevice,
    /// device does not support MMC
    MmcNotSupported,
}

/// could not find any devices that support MMC
#[non_exhaustive]
#[derive(Debug, Display, Error)]
pub struct MmcNotFoundError;

/// could not perform operation on the MMC device
#[non_exhaustive]
#[derive(Debug, Display, Error)]
pub struct MmcOperationError;

/// Error and status information returned by an MMC device.
///
/// Source:
/// SPC-3 > General Concepts > Sense data > Fixed format sense data.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MmcSenseData {
    /// Sense Key (SK) represents generic information describing an exception.
    pub sense_key: SenseKey,

    /// Additional Sense Code (ASC) indicates further information related
    /// to the exception reported by `sense_key`.
    pub asc: u8,

    /// Additional Sense Code Qualifier (ASCQ) indicates detailed information related
    /// to the `additional_sense_code`.
    pub ascq: u8,

    /// Incorrect Length Indicator.
    pub ili: bool,

    /// Command Specific Information indicates info that depends on the command
    /// on which the exception occured.
    pub csi: [u8; 4],

    /// Field Replaceable Unit Code identifies a component that has failed.
    pub fruc: u8,

    /// Sense Key Specific indicates additional information about the exception.
    pub sks: [u8; 3],

    /// Additional Sense Bytes may contain vendor specific data that further
    /// define the exception.
    pub asb: [u8; 46],
}

impl Default for MmcSenseData {
    fn default() -> Self {
        Self {
            sense_key: Default::default(),
            asc: Default::default(),
            ascq: Default::default(),
            ili: Default::default(),
            csi: Default::default(),
            fruc: Default::default(),
            sks: Default::default(),
            asb: [0; _],
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, FromPrimitive)]
pub enum SenseKey {
    /// No sense condition.
    NoSense = 0x0,

    /// The command completed successfully, but some recovery action was taken.
    RecoveredError = 0x1,

    /// The logical unit is not ready to receive the command.
    NotReady = 0x2,

    /// The medium (disk/tape) is defective or the data is unreadable.
    MediumError = 0x3,

    /// A non-recoverable hardware failure occurred.
    HardwareError = 0x4,

    /// An invalid field in the CDB or an unsupported command was sent.
    IllegalRequest = 0x5,

    /// The device has a condition that needs the host's attention
    /// (e.g., medium changed).
    UnitAttention = 0x6,

    /// A command that reads or writes the medium was attempted on a protected
    /// block.
    DataProtect = 0x7,

    /// A write-once or sequential-access device encountered blank medium or
    /// format-defined end-of-data indication while reading or writing.
    BlankCheck = 0x8,

    /// Vendor specific conditions.
    VendorSpecific = 0x9,

    /// An `EXTENDED COPY` command was aborted due to an error condition on
    /// either the source or destination device.
    CopyAborted = 0xA,

    /// The device server aborted the command.
    AbortedCommand = 0xB,

    /// A buffered SCSI device has reached end-of-partition.
    VolumeOverflow = 0xD,

    /// The source data did not match the data read from the medium.
    Miscompare = 0xE,

    /// Unknown sense key.
    #[num_enum(catch_all)]
    Unknown(u8),
}

#[allow(clippy::derivable_impls)] // `num_enum` doesn't work with `#[derive(Default)]`
impl Default for SenseKey {
    fn default() -> Self {
        Self::NoSense
    }
}

/// Direction of MMC data transfer
// The casts are safe since the C enums have implicit discriminants,
// which should be small and non-negative.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum MmcDirection {
    #[default]
    Read = libcdio_sys::mmc_direction_s_SCSI_MMC_DATA_READ as _,
    #[allow(unused)]
    Write = libcdio_sys::mmc_direction_s_SCSI_MMC_DATA_WRITE as _,
}

/// error performing MMC command
#[non_exhaustive]
#[derive(Debug, Display, Error)]
pub enum MmcError {
    /// terminated with `CHECK CONDITION`, sense_key: {0.sense_key:?}, asc: 0x{0.asc:x}, ascq: 0x{0.ascq:x}
    CheckCondition(MmcSenseData),

    /// operating system error
    Os(OsError),
}

/// operating system error
#[repr(i32)]
#[non_exhaustive]
#[derive(Debug, Display, Error, FromPrimitive)]
pub enum OsError {
    /// other error: {0}
    #[num_enum(catch_all)]
    Other(i32),
    /// unsupported operation
    Unsupported = libcdio_sys::driver_return_code_t_DRIVER_OP_UNSUPPORTED,
    /// operation not permitted
    OperationNotPermitted = libcdio_sys::driver_return_code_t_DRIVER_OP_NOT_PERMITTED,
    /// bad parameter
    BadParameter = libcdio_sys::driver_return_code_t_DRIVER_OP_BAD_PARAMETER,
}

/// Implemented MMC commands and their operation codes.
#[repr(u8)]
#[derive(Clone, Copy, Debug)]
enum MmcCommand {
    #[allow(unused)]
    GetConfiguration = 0x46,
    Inquiry = 0x12,
    PreventAllowMediumRemoval = 0x1E,
    ReadDiscInfo = 0x51,
    ReadToc = 0x43,
    SetCdSpeed = 0xBB,
    StartStopUnit = 0x1B,
    TestUnitReady = 0x00,
}

const LEADOUT_TRACK: u8 = 0xAA; // Indicates the end of the disc.

#[cfg(test)]
mod tests {
    use tracing::info;

    use super::*;

    #[test]
    #[ignore = "requires a disc drive with mmc"]
    fn with_device() {
        Mmc::with_device(PathBuf::from("/dev/cdrom")).unwrap();
    }
    #[test]
    #[ignore = "requires a disc drive with mmc"]
    fn level() {
        Mmc::new().unwrap().level().unwrap();
    }

    #[test_log::test(test)]
    #[ignore = "requires a disc drive with mmc"]
    fn sense_data() {
        let mmc = Mmc::new().unwrap();
        // perform an invalid `READ TOC`
        let mut cdb = Cdb::default();
        cdb[0] = 0x43;
        cdb[2] = 0xFF; // invalid value
        mmc.run_command(Some(crate::mmc::MmcDirection::Write), &mut [], cdb)
            .unwrap_err();

        let sense_data = mmc.sense_data().unwrap();
        info!(?sense_data);
    }
}
