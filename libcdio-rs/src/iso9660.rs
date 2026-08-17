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

//! ISO 9660 filesystem related routines.

mod ds;
mod entry;
mod rock;
mod util;
mod xa;

pub use entry::*;
pub use rock::*;
pub use xa::*;

use std::{
    ffi::{CStr, CString, c_char},
    path::Path,
    ptr::{self, NonNull},
};

use bitflags::bitflags;
use libcdio_sys::{
    iso_extension_enum_s_ISO_EXTENSION_HIGH_SIERRA,
    iso_extension_enum_s_ISO_EXTENSION_JOLIET_LEVEL1,
    iso_extension_enum_s_ISO_EXTENSION_JOLIET_LEVEL2,
    iso_extension_enum_s_ISO_EXTENSION_JOLIET_LEVEL3,
    iso_extension_enum_s_ISO_EXTENSION_ROCK_RIDGE, iso9660_t,
};
use num_enum::{IntoPrimitive, TryFromPrimitive};

use crate::logging::init_logger;

/// The main ISO 9660 type
pub struct Iso {
    pub(crate) ptr: NonNull<iso9660_t>,
}

impl Iso {
    /// The number of bytes used by an ISO 9660 block.
    pub const BLOCK_SIZE: usize = 2048;

    /// Open an ISO 9660 image for reading at given `path`, with all iso9660
    /// extension flags enabled. Returns `None` on error.
    pub fn new(path: &Path) -> Option<Self> {
        let path = CString::new(path.to_str()?).ok()?;

        Self::open(&path, IsoExtensions::all())
    }

    fn open(path: &CStr, extensions: IsoExtensions) -> Option<Self> {
        init_logger();

        // SAFETY: path is duplicated by the method, so its safe to drop afterwards
        let iso9660_ptr =
            unsafe { libcdio_sys::iso9660_open_ext(path.as_ptr(), extensions.bits()) };

        Some(Self {
            ptr: NonNull::new(iso9660_ptr)?,
        })
    }

    /// Returns a builder object. See [`IsoBuilder`].
    pub fn builder<'a>(path: &'a Path) -> IsoBuilder<'a> {
        IsoBuilder::new(path)
    }

    /// Returns the Application Identifier.
    pub fn application(&self) -> Option<String> {
        self.get_identifier(libcdio_sys::iso9660_ifs_get_application_id)
    }

    /// Helper for the methods that return iso9660 identifiers.
    fn get_identifier(
        &self,
        func: unsafe extern "C" fn(*mut iso9660_t, *mut *mut c_char) -> bool,
    ) -> Option<String> {
        let mut identifier_ptr = ptr::null_mut();

        // SAFETY: The method allocates a string and points the identifier_ptr to it.
        // It must be freed after use.
        let success = unsafe { func(self.ptr.as_ptr(), &raw mut identifier_ptr) };
        if !success || identifier_ptr.is_null() {
            return None;
        }

        let identifier = unsafe { CStr::from_ptr(identifier_ptr) };
        let identifier = identifier.to_string_lossy().to_string();

        // SAFETY: application_id has been duplicated into a Rust string
        // above, thus safe to free
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
    /// # Note
    /// [`Self`] must be constructed with the joliet extension enabled,
    /// otherwise this will return `None` even if the file has Joliet.
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

/// A builder for [Iso].
#[derive(Clone, Debug)]
pub struct IsoBuilder<'a> {
    extensions: IsoExtensions,
    path: &'a Path,
}

impl<'a> IsoBuilder<'a> {
    pub fn new(path: &'a Path) -> Self {
        Self {
            path,
            extensions: IsoExtensions::empty(),
        }
    }

    /// Set the extensions to be activated. This is set to be empty by default.
    pub fn extensions(mut self, extensions: IsoExtensions) -> Self {
        self.extensions = extensions;
        self
    }

    /// Build the iso9660 type with the set options.
    /// Returns `None` on error.
    pub fn build(self) -> Option<Iso> {
        let path = CString::new(self.path.to_str()?).ok()?;

        Iso::open(&path, self.extensions)
    }
}

bitflags! {
    /// ISO 9660 Extensions.
    /// # Examples
    /// ```rust, no_run
    /// use libcdio_rs::iso9660::IsoExtensions;
    /// // pick HighSierra and RockRidge
    /// let extensions = IsoExtensions::HighSierra & IsoExtensions::RockRidge;
    /// // pick everything except RockRidge
    /// let extensions = IsoExtensions::all() - IsoExtensions::RockRidge;
    /// // pick nothing
    /// let extensions = IsoExtensions::empty();
    /// ```
    #[derive(Clone, Copy, Debug)]
    pub struct IsoExtensions: u8 {
        const HighSierra = iso_extension_enum_s_ISO_EXTENSION_HIGH_SIERRA as _;
        const JolietLevel1 = iso_extension_enum_s_ISO_EXTENSION_JOLIET_LEVEL1 as _;
        const JolietLevel2 = iso_extension_enum_s_ISO_EXTENSION_JOLIET_LEVEL2 as _;
        const JolietLevel3 = iso_extension_enum_s_ISO_EXTENSION_JOLIET_LEVEL3 as _;
        const RockRidge = iso_extension_enum_s_ISO_EXTENSION_ROCK_RIDGE as _;
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

    pub fn test_rockridge_file() -> &'static Path {
        Path::new("../test-data/rock-ridge.iso")
    }
    pub fn test_joliet_file() -> &'static Path {
        Path::new("../test-data/joliet.iso")
    }

    #[test_log::test(test)]
    fn new() {
        let iso = Iso::new(test_rockridge_file());
        assert!(iso.is_some());
    }

    #[test]
    fn builder() {
        let extensions = IsoExtensions::HighSierra & IsoExtensions::RockRidge;
        let iso = Iso::builder(test_rockridge_file())
            .extensions(extensions)
            .build();
        assert!(iso.is_some());
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
