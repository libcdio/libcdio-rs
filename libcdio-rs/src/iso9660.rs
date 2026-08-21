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

//! Routines related to the ISO 9660 filesystem.

pub use entry::*;
pub use rock::*;
pub use xa::*;

mod entry;
mod rock;
mod util;
mod xa;

use std::{
    error::Error,
    ffi::{CStr, CString, OsString, c_char},
    path::{Path, PathBuf},
    ptr::{self, NonNull},
};

use libcdio_sys::iso9660_t;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use thiserror::Error;
use tracing::error;

use crate::logging::init_logger;

/// An ISO 9660 filesystem instance.
pub struct Iso {
    pub(crate) ptr: NonNull<iso9660_t>,
}

impl Iso {
    /// The number of bytes used by an ISO 9660 block.
    pub const BLOCK_SIZE: usize = 2048;

    /// Opens an ISO 9660 image at given `path`.
    pub fn new(path: PathBuf) -> Result<Self, IsoOpenError> {
        init_logger();

        let path = CString::new(path.into_os_string().as_encoded_bytes())
            .inspect_err(|err| error!(%err, "invalid ISO 9660 path"))
            .map_err(|err| IsoOpenError::new(err.clone().into_vec(), err.into()))?;
        let iso9660_ptr = unsafe {
            // enable all extensions
            libcdio_sys::iso9660_open_ext(
                path.as_ptr(),
                (libcdio_sys::iso_extension_enum_s_ISO_EXTENSION_HIGH_SIERRA
                    | libcdio_sys::iso_extension_enum_s_ISO_EXTENSION_JOLIET_LEVEL1
                    | libcdio_sys::iso_extension_enum_s_ISO_EXTENSION_JOLIET_LEVEL2
                    | libcdio_sys::iso_extension_enum_s_ISO_EXTENSION_JOLIET_LEVEL3
                    | libcdio_sys::iso_extension_enum_s_ISO_EXTENSION_ROCK_RIDGE)
                    as _,
            )
        };

        NonNull::new(iso9660_ptr)
            .map(|ptr| Self { ptr })
            .ok_or_else(|| {
                IsoOpenError::new(path.into_bytes(), "iso9660_open_ext() returned NULL".into())
            })
    }

    /// Returns the Application Identifier.
    pub fn application(&self) -> Option<String> {
        self.get_identifier(libcdio_sys::iso9660_ifs_get_application_id)
    }

    /// Helper for the methods that return ISO 9660 identifiers.
    fn get_identifier(
        &self,
        func: unsafe extern "C" fn(*mut iso9660_t, *mut *mut c_char) -> bool,
    ) -> Option<String> {
        let mut identifier_ptr = ptr::null_mut();

        // SAFETY: identifier_ptr must be freed after use.
        let success = unsafe { func(self.ptr.as_ptr(), &raw mut identifier_ptr) };
        if !success || identifier_ptr.is_null() {
            return None;
        }

        let identifier = unsafe { CStr::from_ptr(identifier_ptr) };
        let identifier = identifier.to_string_lossy().to_string();

        // SAFETY: identifier_ptr is already copied to a Rust string.
        unsafe {
            libcdio_sys::cdio_free(identifier_ptr.cast());
        }

        Some(identifier)
    }

    /// Returns the Data Preparer Identifier.
    pub fn data_preparer(&self) -> Option<String> {
        self.get_identifier(libcdio_sys::iso9660_ifs_get_preparer_id)
    }

    /// Returns the Publisher Identifier.
    pub fn publisher(&self) -> Option<String> {
        self.get_identifier(libcdio_sys::iso9660_ifs_get_publisher_id)
    }

    /// Returns the System Identifier.
    pub fn system(&self) -> Option<String> {
        self.get_identifier(libcdio_sys::iso9660_ifs_get_system_id)
    }

    /// Returns the Volume Identifier.
    pub fn volume(&self) -> Option<String> {
        self.get_identifier(libcdio_sys::iso9660_ifs_get_volume_id)
    }

    /// Returns the Volume Set Identifier.
    pub fn volume_set(&self) -> Option<String> {
        self.get_identifier(libcdio_sys::iso9660_ifs_get_volumeset_id)
    }

    /// Returns the Joliet level.
    pub fn joliet_level(&self) -> Option<JolietLevel> {
        let joliet_level = unsafe { libcdio_sys::iso9660_ifs_get_joliet_level(self.ptr.as_ptr()) };
        if joliet_level == 0 {
            return None;
        }
        let joliet_level = JolietLevel::try_from(joliet_level)
            .expect("iso9660_ifs_get_joliet_level() should return a valid joliet level");

        Some(joliet_level)
    }
}

impl Drop for Iso {
    fn drop(&mut self) {
        let _ = unsafe { libcdio_sys::iso9660_close(self.ptr.as_ptr()) };
    }
}

#[derive(Debug, Error)]
#[error(transparent)]
pub struct IsoOpenError(Box<OpenErrRepr>);

#[derive(Debug, Error)]
#[error("could not open ISO 9660 file at `{path}`")]
struct OpenErrRepr {
    path: PathBuf,
    source: Box<dyn Error + Send + Sync>,
}

impl IsoOpenError {
    /// Returns the path of the ISO 9660 file.
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

/// Joliet level.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
pub enum JolietLevel {
    One = 1,
    Two,
    Three,
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn test_rockridge_file() -> PathBuf {
        PathBuf::from("../test-data/rock-ridge.iso")
    }
    pub fn test_joliet_file() -> PathBuf {
        PathBuf::from("../test-data/joliet.iso")
    }

    #[test_log::test(test)]
    fn new() {
        Iso::new(test_rockridge_file()).unwrap();
    }

    #[test]
    fn joliet_level() {
        let iso = Iso::new(test_joliet_file()).unwrap();
        assert_eq!(iso.joliet_level().unwrap(), JolietLevel::Three);
    }

    #[test]
    fn application() {
        let iso = Iso::new(test_rockridge_file()).unwrap();
        assert_eq!(
            &iso.application().unwrap(),
            "K3B THE CD KREATOR VERSION 0.11.20 (C) 2003 SEBASTIAN TRUEG AND THE K3B TEAM"
        );
    }

    #[test]
    fn data_preparer() {
        let iso = Iso::new(test_rockridge_file()).unwrap();
        assert_eq!(&iso.data_preparer().unwrap(), "K3b - Version 0.11.20",);
    }

    #[test]
    fn publisher() {
        let iso = Iso::new(test_rockridge_file()).unwrap();
        assert_eq!(&iso.publisher().unwrap(), "Rocky Bernstein");
    }

    #[test]
    fn system() {
        let iso = Iso::new(test_rockridge_file()).unwrap();
        assert_eq!(&iso.system().unwrap(), "LINUX");
    }

    #[test]
    fn volume() {
        let iso = Iso::new(test_rockridge_file()).unwrap();
        assert_eq!(&iso.volume().unwrap(), "Rock Ridge Copy test");
    }

    #[test]
    fn volume_set() {
        let iso = Iso::new(test_rockridge_file()).unwrap();
        assert!(&iso.volume_set().is_none());
    }
}
