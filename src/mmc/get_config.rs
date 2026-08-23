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

//! SCSI MMC (MultiMedia Commands) GET CONFIGURATION command.

use displaydoc::Display;
use num_enum::TryFromPrimitive;
use tracing::{debug, error};

use crate::mmc::{Mmc, MmcOperationError};

/// Methods related to the `GET CONFIGURATION` command.
impl Mmc {
    /// Get a list of MMC features supported by the device.
    ///
    /// Returns `None` on error or if the device is unavailable.
    pub fn features(&self) -> Result<Vec<MmcFeature>, MmcOperationError> {
        let mut buf = [0_u8; ALLOC_BUF_SIZE];
        let retval = unsafe {
            libcdio_sys::mmc_get_configuration(
                self.cdio.cdio.as_ptr(),
                buf.as_mut_ptr().cast(),
                buf.len() as u32,
                GET_CONF_RET_TYPE_ZERO,
                STARTING_FEATURE_NUMBER,
                TIMEOUT_MILLIS,
            )
        };
        if retval != 0 {
            error!(retval, "non success code from mmc_get_configuration()");
            return Err(MmcOperationError);
        };

        let dtors_len = read_u32(&buf) as usize;
        // data length (len) doesn't include its own length..
        let expected = dtors_len + 4;
        if buf.len() < expected {
            error!(
                expected,
                len = buf.len(),
                "insufficient buffer length for mmc response data",
            );
            return Err(MmcOperationError);
        }
        // feature descriptors
        let dtors = &buf[8..8 + dtors_len - 4];

        let mut features = Vec::new();
        let mut i = 0;
        while i + 3 < dtors.len() {
            let dtor_len = usize::from(dtors[i + 3]) + 4;
            let dtor = &dtors[i..i + dtor_len];
            if let Some(feature) = MmcFeature::parse(dtor) {
                features.push(feature);
            };
            i += dtor_len;
        }

        /// Return type to request all features
        const GET_CONF_RET_TYPE_ZERO: u32 = 0x0;
        const ALLOC_BUF_SIZE: usize = 4 * 1024;
        const TIMEOUT_MILLIS: u32 = 6000;
        /// Starting feature set to profile list, to get all features
        const STARTING_FEATURE_NUMBER: u32 = 0;

        Ok(features)
    }
}

// WARNING: Changes to the doc comments can affect the type's display output!
/// A set of commands and behaviours that specify the capabilities of a drive
/// and its associated medium.
#[non_exhaustive]
#[derive(Clone, Debug, Default, Display, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[ignore_extra_doc_attributes]
pub enum MmcFeature {
    /// Profile List
    ///
    /// A list of all profiles supported by the drive
    ProfileList { profiles: Vec<MmcProfile> },

    /// Core
    ///
    /// Mandatory behavior for all devices
    Core {
        /// Physical interface standard reported by MMC.
        /// <div class="warning">
        ///
        /// **NOTE**: It is possible that more than one physical interface exists between the Host and Drive, e.g., an
        /// IEEE1394 Host connecting to an ATAPI bridge to an ATAPI Drive. The Drive may not be aware of
        /// interfaces beyond the ATAPI.
        ///
        /// </div>
        interface: MmcInterface,
    },

    /// Morphing
    ///
    /// Ability to report operational changes to the host and accept requests to
    /// prevent operational changes
    Morphing {
        /// Supports async in addition to polling implementations of
        /// `GET EVENT STATUS NOTIFICATION`
        async_events: bool,
        /// Supports Operational Change Request/Notification Class Events
        op_chg_events: bool,
    },

    /// Removable Medium
    ///
    /// Ability to remove the medium from the device
    RemovableMedium {
        /// The drive is capable of ejecting media via `START/STOP` commands
        eject: bool,
        /// The drive is capable of locking media
        lock: bool,
        /// The drive has a prevent jumper
        prevent_jumper: bool,
        /// The Loading mechanism type used by the drive
        load_mech: MmcLoadMech,
    },

    /// Write Protect
    ///
    /// Ability to control write protection status
    WriteProtect,

    /// Random Readable
    ///
    /// Ability to read sectors with random addressing
    RandomReadable,

    /// Multi-Read
    ///
    /// The drive is able to read all CD media types; based on OSTA MultiRead
    MultiRead,

    /// CD Read
    ///
    /// Ability to read CD-specific structures
    CdRead {
        /// The feature is currently active
        active: bool,
        /// Supports C2 error pointers
        c2_error: bool,
        /// Supports CD-Text (Format Code `5h` of `READ TOC/PMA/ATIP`)
        cd_text: bool,
        /// Supports DAP bit for the `CDB` in `READ CD` and `READ CD MSF` commands
        dap: bool,
    },

    /// DVD Read
    ///
    /// Ability to read DVD-specific structures
    DvdRead,

    /// Random Writable
    ///
    /// Write support for randomly addressed writes
    RandomWritable,

    /// Incremental Streaming Writable
    ///
    /// Write support for sequential recording
    IncrementalStreamingWritable,

    /// Sector Erasable
    ///
    /// Write support for erasable media and media that requires an erase pass
    /// before overwrite
    SectorErasable,

    /// Formattable
    ///
    /// Support for formatting of media
    Formattable,

    /// Hardware Defect Management
    ///
    /// Ability of the drive/media system to provide an apparently defect-free
    /// space
    HardwareDefectManagement,

    /// Write Once
    ///
    /// Write support for write-once media that is writable in random order
    WriteOnce,

    /// Restricted Overwrite
    ///
    /// Write support for media that shall be written from blocking boundaries
    /// only
    RestrictedOverwrite,

    /// CD-RW CAV Write
    ///
    /// Ability to write high-speed CD-RW media
    CdRwCavWrite,

    /// MRW
    ///
    /// Ability to recognize and read and optionally write MRW formatted
    /// media
    Mrw,

    /// Enhanced Defect Reporting
    ///
    /// Ability to control `RECOVERED ERROR` reporting
    EnhancedDefectReporting,

    /// Ability to recognize, read and optionally write DVD+RW media
    DvdPlusRw,

    /// DVD+R
    ///
    /// Ability to read DVD+R recorded media formats
    DvdPlusR,

    /// Rigid Restricted Overwrite
    ///
    /// Write support for media that is required to be written from Blocking
    /// boundaries with length of integral multiple of Blocking size only.
    RigidRestrictedOverwrite,

    /// CD Track At Once
    ///
    /// Ability to write CD with Track at Once recording
    CdTrackAtOnce,

    /// CD Mastering
    ///
    /// Ability to write CD with Session at Once or Raw write methods
    CdMastering,

    /// DVD-R/-RW Write
    ///
    /// Ability to write DVD specific structures
    DvdRRwWrite,

    /// DDCD Read
    ///
    /// Ability to read user data from DDCD blocks
    DdcdRead,

    /// DDCD-R Write
    ///
    /// Ability to write and read DDCD-R media
    DdcdRWrite,

    /// DDCD-RW Write
    ///
    /// Ability to write and read DDCD-RW media
    DdcdRwWrite,

    /// Layer Jump Recording
    ///
    /// Ability to record in layer jump mode
    LayerJumpRecording,

    /// Layer Jump Rigid Restricted Overwrite
    ///
    /// Ability to perform Layer Jump recording on Rigid Restricted
    /// Overwritable media
    LayerJumpRigidRestrictedOverwrite,

    /// Stop Long Operation
    ///
    /// Ability to stop the long immediate operation by a command
    StopLongOperation,

    /// CD-RW Media Write Support
    ///
    /// Ability to report CD-RW media sub-types that are supported
    /// for write
    CdRwMediaWriteSupport,

    /// BD-R POW
    ///
    /// Logical Block overwrite service on BD-R discs formatted as SRM+POW
    BdRPow,

    /// DVD+RW Dual Layer
    ///
    /// Ability to read DVD+RW Dual Layer recorded media formats
    DvdPlusRwDualLayer,

    /// DVD+R Dual Layer
    ///
    /// Ability to read DVD+R Dual Layer recorded media formats
    DvdPlusRDualLayer,

    /// BD Read
    ///
    /// Ability to read control structures and user data from a BD disc
    BdRead,

    /// BD Write
    ///
    /// Ability to write control structures and user data to certain BD
    /// discs
    BdWrite,

    /// Timely Safe Recording
    ///
    /// Timely, Safe Recording permits the Host to schedule defect management
    Tsr,

    /// HD DVD Read
    ///
    /// Ability to read control structures and user data from a HD DVD disc
    HdDvdRead,

    /// HD DVD Write
    ///
    /// Ability to write control structures and user data from a HD DVD disc
    HdDvdWrite,

    /// HD DVD-RW Fragment
    ///
    /// HD DVD-RW fragment recording
    HdDvdRwFragment,

    /// Hybrid Disc
    ///
    /// Ability to access some hybrid discs
    HybridDisc,

    /// Power Management
    ///
    /// Host and device directed power management
    PowerManagement,

    /// SMART
    ///
    /// Ability to perform Self Monitoring Analysis and Reporting Technology
    Smart,

    /// Embedded Changer
    ///
    /// Single mechanism multiple disc changer
    EmbeddedChanger,

    /// CD Audio External Play
    ///
    /// Ability to play audio CDs via the Logical Unit’s own analog output
    CdAudioExternalPlay {
        /// Feature is currently active
        active: bool,
        /// Supports the `SCAN` command
        scan: bool,
        /// Supports independent mute of audio channels
        sep_channel_mute: bool,
        /// Supports independent volume levels for audio channels
        sep_volume: bool,
        /// Number of discrete volume levels supported
        volume_levels: u16,
    },

    /// Microcode Upgrade
    ///
    /// Ability for the device to accept new microcode via the interface
    MicrocodeUpgrade,

    /// Timeout
    ///
    /// Ability to respond to all commands within a specific time
    Timeout,

    /// DVD-CSS
    ///
    /// Ability to perform DVD CSS/CPPM authentication and RPC
    DvdCss {
        /// Feature is currently active (DVD CSS/CPPM media is present)
        active: bool,
        /// CSS Version
        version: u8,
    },

    /// Real Time Streaming
    ///
    /// Ability to read and write using host requested performance parameters
    RealTimeStreaming,

    /// Drive Serial Number
    ///
    /// The drive has a unique identifier
    DriveSerialNumber { sno: String },

    /// Media Serial Number
    ///
    /// Ability to return unique Media Serial Number
    MediaSerialNumber,

    /// Disc Control Blocks
    ///
    /// Ability to read and/or write DCBs
    DiscControlBlocks,

    /// DVD CPRM
    ///
    /// Ability to perform DVD CPRM authentication
    DvdCprm,

    /// Firmware Information
    ///
    /// Firmware creation date report
    FirmwareInformation,

    /// AACS
    ///
    /// Ability to decode and optionally encode AACS protected information
    Aacs,

    /// DVD CSS Managed Recording
    ///
    /// Ability to perform DVD CSS managed recording
    DvdCssManagedRecording,

    /// VCPS
    ///
    /// Ability to decode and optionally encode VCPS protected information
    Vcps,

    /// SecurDisc
    ///
    /// Ability to encode and decode SecurDisc protected information
    ///
    SecurDisc,

    /// OSSC
    ///
    /// TCG Optical Security Subsystem Class feature
    Ossc,

    /// Vendor Specific
    ///
    /// Vendor-specific feature
    #[default]
    VendorSpecific,
}

impl MmcFeature {
    /// Parse a feature from a slice pointing to a feature descriptor.
    fn parse(dtor: &[u8]) -> Option<Self> {
        if dtor.len() < 4 {
            error!(
                len = dtor.len(),
                "mmc feature descriptor buffer must be atleast 4 bytes",
            );
            return None;
        }
        let data_len = usize::from(dtor[3]);
        let expected = data_len + 4;
        if dtor.len() != expected {
            debug!(
                expected,
                len = dtor.len(),
                "mmc feature descriptor buffer has insufficient size",
            );
            return None;
        };

        let code = read_u16(dtor);
        match code {
            0x0000 => Some(Self::profile_list(dtor)),
            0x0001 => Self::core(dtor),
            0x0002 => Some(Self::morphing(dtor)),
            0x0003 => Self::medium(dtor),
            0x001e => Some(Self::cd_read(dtor)),
            0x0103 => Some(Self::cd_audio_external_play(dtor)),
            0x0106 => Some(Self::dvd_css(dtor)),
            0x0108 => Self::drive_serial_num(dtor),
            _ => None,
        }
    }

    fn profile_list(dtor: &[u8]) -> Self {
        let data = &dtor[4..]; // skip the header
        let profiles = data
            .chunks_exact(4)
            .filter_map(|chunk| {
                let kind = read_u16(chunk);
                Some(MmcProfile {
                    active: chunk[2] & 0b1 != 0,
                    kind: ProfileKind::try_from(kind)
                        .inspect_err(|err| error!(?err, kind, "invalid profile kind from mmc"))
                        .ok()?,
                })
            })
            .collect();

        MmcFeature::ProfileList { profiles }
    }

    fn core(dtor: &[u8]) -> Option<Self> {
        let interface = read_u32(&dtor[4..]);
        Some(MmcFeature::Core {
            interface: MmcInterface::try_from(interface)
                .inspect_err(|err| error!(?err, interface, "got invalid interface value from mmc"))
                .ok()?,
        })
    }

    fn morphing(dtor: &[u8]) -> Self {
        Self::Morphing {
            async_events: dtor[4] & 1 != 0,
            op_chg_events: dtor[4] & 1 << 1 != 0,
        }
    }

    fn medium(dtor: &[u8]) -> Option<Self> {
        let load_mech = dtor[4] >> 5;
        let load_mech = MmcLoadMech::try_from(load_mech)
            .inspect_err(|err| error!(?err, load_mech, "got invalid loading mech value from mmc"))
            .ok()?;

        Some(Self::RemovableMedium {
            eject: dtor[4] & 1 << 3 != 0,
            lock: dtor[4] & 1 != 0,
            prevent_jumper: dtor[4] & 1 << 2 == 0,
            load_mech,
        })
    }

    fn cd_read(data: &[u8]) -> Self {
        Self::CdRead {
            active: data[2] & 1 != 0,
            c2_error: data[4] & 1 != 0,
            cd_text: data[4] & 1 << 1 != 0,
            dap: data[4] & 1 << 7 != 0,
        }
    }

    fn cd_audio_external_play(dtor: &[u8]) -> Self {
        Self::CdAudioExternalPlay {
            active: dtor[2] & 1 != 0,
            scan: dtor[4] & 1 << 2 != 0,
            sep_channel_mute: dtor[4] & 1 << 1 != 0,
            sep_volume: dtor[4] & 1 != 0,
            volume_levels: read_u16(&dtor[6..]),
        }
    }

    fn dvd_css(dtor: &[u8]) -> Self {
        Self::DvdCss {
            active: dtor[2] & 1 != 0,
            version: dtor[7],
        }
    }

    fn drive_serial_num(dtor: &[u8]) -> Option<Self> {
        let current = dtor[2] & 1 != 0;
        if !current {
            return None;
        }
        let len = usize::from(dtor[3]);
        Some(Self::DriveSerialNumber {
            sno: String::from_utf8(dtor[4..4 + len].to_vec())
                .inspect_err(|err| {
                    error!(?err, "could not make a string from mmc drive serial number")
                })
                .ok()?,
        })
    }
}

/// A base set of functions for specific drive/media combination.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MmcProfile {
    /// Profile's current bit is set
    pub active: bool,
    /// Profile type
    pub kind: ProfileKind,
}
// WARNING: Changes to the doc comments can affect the type's display output!
/// Represents the complete set of MMC feature profiles for optical disc drives.
/// Each variant corresponds to a specific media type and recording capability.
#[repr(u16)]
#[non_exhaustive]
#[derive(
    Clone, Copy, Debug, Default, Display, Eq, Hash, Ord, PartialEq, PartialOrd, TryFromPrimitive,
)]
pub enum ProfileKind {
    /// Non-removable disk
    NonRemovable = 0x0001,

    /// Removable disk
    Removable = 0x0002,

    /// Magneto-Optical Erasable disk
    MoErasable = 0x0003,

    /// Optical Write-Once
    OpticalWriteOnce = 0x0004,

    /// Advance Storage - Magneto-Optical
    AsMo = 0x0005,

    /// CD-ROM
    CdRom = 0x0008,

    /// CD-R
    CdR = 0x0009,

    /// CD-RW
    CdRw = 0x000A,

    /// DVD-ROM
    DvdRom = 0x0010,

    /// DVD-R Sequential Recording
    DvdRSeqRec = 0x0011,

    /// DVD-RAM
    DvdRam = 0x0012,

    /// DVD-RW Restricted Overwrite
    DvdRwRo = 0x0013,

    /// DVD-RW Sequential Recording
    DvdRwSeqRec = 0x0014,

    /// DVD-R Dual Layer Sequential recording
    DvdRDlSeqRec = 0x0015,

    /// DVD-R Dual Layer Jump Recording
    DvdRDlJmpRec = 0x0016,

    /// DVD-RW Dual Layer
    DvdRwDl = 0x0017,

    /// DVD-Download Disc Recording
    DvdDownDiscRec = 0x0018,

    /// DVD+RW
    DvdPlusRw = 0x001A,

    /// DVD+R
    DvdPlusR = 0x001B,

    /// DDCD-ROM
    DdcdRom = 0x0020,

    /// DDCD-R
    DdcdR = 0x0021,

    /// DDCD-RW
    DdcdRw = 0x0022,

    /// DVD+RW Dual Layer
    DvdPlusRwDl = 0x002A,

    /// DVD+R Double Layer
    DvdPlusRDl = 0x002B,

    /// BD-ROM
    BdRom = 0x0040,

    /// BD-R Sequential Recording Mode
    BdRSeqRec = 0x0041,

    /// BD-R Random Recording Mode
    BdRRandRec = 0x0042,

    /// BD-RE
    BdRw = 0x0043,

    /// HD DVD-ROM
    HdDvdRom = 0x0050,

    /// HD DVD-R
    HdDvdR = 0x0051,

    /// HD DVD-RAM
    HdDvdRam = 0x0052,

    /// HD DVD-RW
    HdDvdRw = 0x0053,

    /// HD DVD-R Dual Layer
    HdDvdRDl = 0x0058,

    /// HD DVD-RW Dual Layer
    HdDvdRwDl = 0x0059,

    /// The Drive does not conform to any Profile
    #[default]
    NonConform = 0xFFFF,
}

// WARNING: Changes to the doc comments can affect the type's display output!
/// Physical interface standard reported by MMC.
#[repr(u32)]
#[non_exhaustive]
#[derive(
    Clone, Copy, Debug, Default, Display, Eq, Hash, Ord, PartialEq, PartialOrd, TryFromPrimitive,
)]
pub enum MmcInterface {
    #[default]
    /// Unspecified
    Unspecified = 0x0,
    /// SCSI
    Scsi = 0x1,
    /// ATAPI
    Atapi = 0x2,
    /// IEEE 1394
    Ieee1394 = 0x3,
    /// IEEE 1394A
    Ieee1394A = 0x4,
    /// Fibre Channel
    FibreChannel = 0x5,
    /// IEEE 1394B
    Ieee1394B = 0x6,
    /// Serial ATAPI
    SerialAtapi = 0x7,
    /// USB (both 1.1 and 2.0)
    Usb = 0x8,
    /// Vendor Unique
    VendorUnique = 0xffff,
}

/// Removable medium info about the drive, reported by MMC.
#[derive(Clone, Copy, Debug)]
pub struct MmcMedium {
    /// The drive is capable of ejecting media via START/STOP commands
    pub eject: bool,
    /// The drive is capable of locking media
    pub lock: bool,
    /// The drive has a prevent jumper
    pub prevent_jumper: bool,
    /// The Loading mechanism type used by the drive
    pub load_mech: MmcLoadMech,
}

// WARNING: Changes to the doc comments can affect the type's display output!
/// Loading mechanism type used by the drive.
#[repr(u8)]
#[non_exhaustive]
#[derive(
    Clone, Copy, Debug, Default, Display, Eq, Hash, Ord, PartialEq, PartialOrd, TryFromPrimitive,
)]
pub enum MmcLoadMech {
    /// Caddy/Slot type
    CaddySlot = 0b000,
    #[default]
    /// Tray type
    Tray = 0b001,
    /// Pop-up type
    PopUp = 0b010,
    /// Embedded changer with individually changeable discs
    EmbeddedChangerIndividualDiscs = 0b100,
    /// Embedded changer using a magazine mechanism
    EmbeddedChangerMagazine = 0b101,
}

/// Parse a big endian u16 out of the next two bytes.
fn read_u16(val: &[u8]) -> u16 {
    let val = *val
        .first_chunk()
        .expect("mmc data buffer should be sufficiently large");
    u16::from_be_bytes(val)
}
/// Parse a big endian u32 out of the next four bytes.
fn read_u32(val: &[u8]) -> u32 {
    let val = *val
        .first_chunk()
        .expect("mmc data buffer should be sufficiently large");
    u32::from_be_bytes(val)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test(test)]
    #[ignore = "requires a disc drive with mmc"]
    fn features() {
        Mmc::new().unwrap().features().unwrap();
    }
}
