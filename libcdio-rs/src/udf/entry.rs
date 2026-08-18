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

//! Routines related to UDF filesystem entries.

use std::{
    error::Error,
    ffi::{CStr, CString},
    io,
    marker::PhantomData,
    ptr::NonNull,
};

use file_mode::Mode;
use libcdio_sys::{udf_dirent_s, udf_t};
use thiserror::Error;
use time::OffsetDateTime;

use crate::udf::Udf;

impl Udf {
    /// Returns the root entry of the UDF filesystem.
    pub fn root(&self) -> Result<UdfEntry<'_>, UdfGetEntryError> {
        // SAFETY: UdfEntry will own the returned value.
        let entry = unsafe { libcdio_sys::udf_get_root(self.udf.as_ptr(), true, 0) };

        NonNull::new(entry)
            .map(UdfEntry::new)
            .ok_or_else(|| UdfGetEntryError::new("/", "udf_get_root() returned NULL".into()))
    }

    /// Returns the root entry of the UDF filesystem, at the given partition.
    pub fn root_from_partition(&self, partition: u16) -> Result<UdfEntry<'_>, UdfGetEntryError> {
        // SAFETY: UdfEntry will own the returned value.
        let entry = unsafe { libcdio_sys::udf_get_root(self.udf.as_ptr(), false, partition) };

        NonNull::new(entry)
            .map(UdfEntry::new)
            .ok_or_else(|| UdfGetEntryError::new("/", "udf_get_root() returned NULL".into()))
    }

    /// Returns UDF entry at `path`.
    ///
    /// Only Unix-style `/` may be used as a path separator.
    pub fn entry(&self, path: String) -> Result<UdfEntry<'_>, UdfGetEntryError> {
        let root = self.root()?;
        let path = CString::new(path).map_err(|err| UdfGetEntryError::new(path, err.into()))?;
        // SAFETY: UdfEntry will own the returned value.
        let entry = unsafe { libcdio_sys::udf_fopen(root.entry.as_ptr(), path.as_ptr()) };

        NonNull::new(entry).map(UdfEntry::new).ok_or_else(|| {
            UdfGetEntryError::new(
                path.into_string()
                    .expect("path was originally a valid string"),
                "udf_fopen() returned NULL".into(),
            )
        })
    }
}

#[derive(Debug, Error)]
#[error(transparent)]
pub struct UdfGetEntryError(Box<GetEntryErrRepr>);

#[derive(Debug, Error)]
#[error("could not get UDF entry at `{path}`")]
struct GetEntryErrRepr {
    path: String,
    source: Box<dyn Error + Send + Sync>,
}

impl UdfGetEntryError {
    /// The path of the UDF entry that caused the error.
    pub fn path(&self) -> &str {
        &self.0.path
    }
    fn new(path: impl Into<String>, source: Box<dyn Error + Send + Sync>) -> Self {
        Self(Box::new(GetEntryErrRepr {
            path: path.into(),
            source,
        }))
    }
}

/// A UDF file/directory entry.
pub struct UdfEntry<'a> {
    entry: NonNull<udf_dirent_s>,
    pub gid: u32,
    pub uid: u32,
    // udf_dirent_s has internal references to its parent udf_t
    _parent: PhantomData<&'a udf_t>,
}

impl UdfEntry<'_> {
    /// Returns the modification time.
    pub fn modify_time(&self) -> Result<OffsetDateTime, UdfInvalidEntryError> {
        // SAFETY: Returns -1 in case the value is invalid, checked immediately below
        let time = unsafe { libcdio_sys::udf_get_modification_time(self.entry.as_ptr()) };
        if time == -1 {
            return Err(UdfInvalidEntryError::new(
                self.filename().ok(),
                "udf_get_modification_time() returned -1".into(),
            ));
        }

        OffsetDateTime::from_unix_timestamp(time)
            .map_err(|err| UdfInvalidEntryError::new(self.filename().ok(), err.into()))
    }

    /// Returns the file name.
    pub fn filename(&self) -> Result<&str, UdfInvalidEntryError> {
        const CURRENT_DIR_FILENAME: &str = ".";

        // SAFETY: self.entry is non null, therefore this method should not return null
        let filename = unsafe { libcdio_sys::udf_get_filename(self.entry.as_ptr()) };
        if filename.is_null() {
            return Err(UdfInvalidEntryError::new(
                Option::<&str>::None,
                "udf_get_filename() returned NULL".into(),
            ));
        }
        let filename = unsafe { CStr::from_ptr(filename) };
        // filename returns an empty string after opening the root directory.
        // this probably represents "."
        if filename.is_empty() {
            return Ok(CURRENT_DIR_FILENAME);
        }

        filename
            .to_str()
            .map_err(|err| UdfInvalidEntryError::new(Option::<&str>::None, err.into()))
    }

    /// Returns the next entry.
    pub fn next(self) -> Option<Self> {
        // SAFETY: This function moves self. Use mem::forget to stop the destructor.
        let next_entry = unsafe { libcdio_sys::udf_readdir(self.entry.as_ptr()) };
        std::mem::forget(self);

        NonNull::new(next_entry).map(Self::new)
    }

    /// Opens `self` and return the first entry.
    pub fn open_dir(&self) -> Option<Self> {
        let sub_entry = unsafe { libcdio_sys::udf_opendir(self.entry.as_ptr()) };

        Some(Self::new(NonNull::new(sub_entry)?))
    }

    /// Checks if the entry is a directory.
    pub fn is_dir(&self) -> bool {
        unsafe { libcdio_sys::udf_is_dir(self.entry.as_ptr()) }
    }

    /// Returns the file length.
    pub fn file_length(&self) -> u64 {
        // SAFETY: entry is not null, making this function infallible
        unsafe { libcdio_sys::udf_get_file_length(self.entry.as_ptr()) }
    }

    /// Returns the POSIX file mode.
    pub fn mode(&self) -> Mode {
        // `mode_t` is non-portable (16 or 32 bit)
        #[allow(clippy::useless_conversion)]
        let mode = u32::from(unsafe { libcdio_sys::udf_get_posix_filemode(self.entry.as_ptr()) });
        Mode::new(mode, u32::MAX)
    }

    /// Returns the number of hard links of the entry.
    pub fn link_count(&self) -> u16 {
        unsafe { libcdio_sys::udf_get_link_count(self.entry.as_ptr()) }
    }

    /// Returns a type that implements [`io::Read`] to allow for reading the
    /// file data of a UDF entry.
    pub fn reader(&self) -> UdfEntryReader<'_> {
        UdfEntryReader {
            bytes_read: 0,
            entry: self,
        }
    }

    fn new(entry: NonNull<udf_dirent_s>) -> Self {
        let uid = unsafe { (*entry.as_ptr()).fe.uid };
        let gid = unsafe { (*entry.as_ptr()).fe.gid };

        Self {
            entry,
            gid: u32::from_le(gid),
            uid: u32::from_le(uid),
            _parent: PhantomData,
        }
    }
}

impl Drop for UdfEntry<'_> {
    fn drop(&mut self) {
        let _ = unsafe { libcdio_sys::udf_dirent_free(self.entry.as_ptr()) };
    }
}

/// UDF entry has invalid data
#[derive(Debug, Error)]
#[error(transparent)]
pub struct UdfInvalidEntryError(Box<InvalidEntryErrRepr>);

#[derive(Debug, Error)]
#[error("found invalid data in UDF entry `{}`", name.as_deref().unwrap_or_default())]
struct InvalidEntryErrRepr {
    name: Option<String>,
    source: Box<dyn Error + Send + Sync>,
}

impl UdfInvalidEntryError {
    /// Returns file name of the entry with invalid data.
    pub fn name(&self) -> Option<&str> {
        self.0.name.as_deref()
    }
    fn new(name: Option<impl Into<String>>, source: Box<dyn Error + Send + Sync>) -> Self {
        Self(Box::new(InvalidEntryErrRepr {
            name: name.map(Into::into),
            source,
        }))
    }
}

/// A type that implements [`io::Read`], to allow for reading the
/// file data of a UDF entry.
// This is NOT thread safe, as udf_dirent_s internally holds
// its current file position with a non-atomic integer
pub struct UdfEntryReader<'a> {
    bytes_read: usize,
    entry: &'a UdfEntry<'a>,
}

impl UdfEntryReader<'_> {
    /// Sets the current file position of the entry.
    fn set_position(&mut self, block_num: usize) {
        // SAFETY: UdfEntryReader and UdfEntry are not marked
        // as thread safe
        let _ = unsafe {
            libcdio_sys::udf_setpos(
                self.entry.entry.as_ptr(),
                (block_num * Udf::BLOCK_SIZE)
                    .try_into()
                    .expect("block's byte offset should fit an `off_t`"),
            )
        };
    }
}

impl io::Read for UdfEntryReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let file_size = self.entry.file_length() as usize;
        let mut buf_read = 0;

        // As of writing, udf_dirent_s stores the current file position
        // at p_udf->i_position. This is used by libcdio's UDF read and UDF
        // seek routines.
        // This causes state leakage when more than one instance of
        // UdfEntryReader are used from the same UdfEntry.
        // Fix this by resetting the file position value to zero before
        // actions that change it.
        self.set_position(0);

        while self.bytes_read < file_size && buf_read < buf.len() {
            let block_num = self.bytes_read / Udf::BLOCK_SIZE;
            self.set_position(block_num);
            let mut block = [0_u8; Udf::BLOCK_SIZE];
            let ret = unsafe {
                libcdio_sys::udf_read_block(self.entry.entry.as_ptr(), block.as_mut_ptr().cast(), 1)
            };
            // cast is safe as Udf::BLOCK_SIZE < i16::MAX
            if ret != block.len() as _ {
                return Err(io::Error::other(format!(
                    "error reading udf block number: {block_num}",
                )));
            }
            let block_start = self.bytes_read % block.len();
            let buf_rem = buf.len() - buf_read;
            let block_rem = (block.len() - block_start).min(file_size - self.bytes_read);
            let len = buf_rem.min(block_rem);
            buf[buf_read..buf_read + len].copy_from_slice(&block[block_start..block_start + len]);
            buf_read += len;
            self.bytes_read += len;
        }

        Ok(buf_read)
    }
}

impl io::Seek for UdfEntryReader<'_> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.bytes_read = match pos {
            io::SeekFrom::Start(offset) => offset as usize,
            io::SeekFrom::End(offset) => {
                self.entry.file_length().saturating_add_signed(offset) as usize
            }
            io::SeekFrom::Current(offset) => self.bytes_read.saturating_add_signed(offset as isize),
        };

        Ok(self.bytes_read as u64)
    }
}

#[cfg(test)]
mod tests {
    use std::{io::Read, path::PathBuf};

    use time::macros::datetime;

    use crate::udf::tests::test_udf_file;

    use super::*;

    fn test_udf_file1() -> PathBuf {
        PathBuf::from("../test-data/udf1.iso")
    }

    #[test]
    fn root() {
        let udf = Udf::new(test_udf_file()).unwrap();
        udf.root().unwrap();
    }

    #[test]
    fn root_from_partition() {
        let udf = Udf::new(test_udf_file()).unwrap();
        udf.root_from_partition(0).unwrap();
    }

    #[test]
    fn modify_time() {
        let udf = Udf::new(test_udf_file()).unwrap();
        let modify_time = udf.root().unwrap().modify_time().unwrap();
        assert_eq!(modify_time, datetime!(2014-02-20 1:26:20.0 +00:00:00));
    }

    #[test]
    fn filename() {
        let udf = Udf::new(test_udf_file()).unwrap();
        let root = udf.root().unwrap();
        assert_eq!(root.filename().unwrap(), "/");
    }

    #[test]
    fn next() {
        let udf = Udf::new(test_udf_file()).unwrap();
        let root = udf.root().unwrap();
        let next = root.next().unwrap();
        assert_eq!(next.filename().unwrap(), ".");

        let next = next.next().unwrap();
        assert_eq!(next.filename().unwrap(), "FéжΘvrier");
    }

    #[test]
    fn is_dir() {
        let udf = Udf::new(test_udf_file()).unwrap();
        let root = udf.root().unwrap();
        assert!(root.is_dir());
    }

    #[test]
    fn file_length() {
        let udf = Udf::new(test_udf_file()).unwrap();
        let root = udf.root().unwrap();
        let file = root.next().unwrap().next().unwrap();
        assert_eq!(file.file_length(), 10);
    }

    #[test]
    fn mode() {
        let udf = Udf::new(test_udf_file()).unwrap();
        let root = udf.root().unwrap();
        let entry = root.next().unwrap();
        let entry = entry.next().unwrap();
        assert_eq!(&entry.mode().to_string(), "-r-xr-xr-x");
    }

    #[test]
    fn link_count() {
        let udf = Udf::new(test_udf_file()).unwrap();
        let root = udf.root().unwrap();
        let entry = root.next().unwrap().next().unwrap();
        assert_eq!(entry.link_count(), 1);
    }

    #[test]
    fn fields() {
        let udf = Udf::new(test_udf_file1()).unwrap();
        let root = udf.root().unwrap();
        let entry = root.next().unwrap().next().unwrap();
        assert_eq!(entry.uid, 2000);
        assert_eq!(entry.gid, 3000);
    }

    #[test]
    fn open_dir() {
        let udf = Udf::new(test_udf_file1()).unwrap();
        let entry = udf.root().unwrap();
        // /licenses
        let entry = entry.next().unwrap().next().unwrap();
        // /licenses/.
        entry.open_dir().unwrap();
    }

    #[test]
    fn read() {
        let udf = Udf::new(test_udf_file1()).unwrap();
        let root = udf.root().unwrap();
        // /licenses
        let entry = root.next().unwrap().next().unwrap();
        // /licenses/.
        let entry = entry.open_dir().unwrap().next().unwrap();
        // /licenses/COPYING
        let entry = entry.next().unwrap();

        let mut reader = entry.reader();
        let mut contents = String::new();
        let bytes_read = reader.read_to_string(&mut contents).unwrap();

        let gpl = std::fs::read_to_string("../COPYING").unwrap();
        assert_eq!(gpl.len(), bytes_read);
        assert_eq!(gpl, contents);
    }

    #[test]
    fn entry() {
        let udf = Udf::new(test_udf_file1()).unwrap();
        udf.entry("/licenses/COPYING.LESSER".to_owned()).unwrap();
    }
}
