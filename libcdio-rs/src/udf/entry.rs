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

//! UDF file/directory entry.

use std::{
    ffi::{CStr, CString},
    io,
    marker::PhantomData,
    ptr::NonNull,
};

use file_mode::Mode;
use libcdio_sys::udf_dirent_s;
use time::OffsetDateTime;

use crate::udf::Udf;

/// A UDF file/directory entry.
pub struct UdfEntry<'a> {
    entry: NonNull<udf_dirent_s>,
    /// The Group ID of the entry
    pub gid: u32,
    /// The User ID of the entry
    pub uid: u32,
    // udf_dirent_s internally holds references to udf_t
    // thus it is valid for only as long as its parent
    // udf_t is
    _phantom: PhantomData<&'a udf_dirent_s>,
}

/// A type that implements [`io::Read`], to allow for reading the
/// file corresponding to a [`UdfEntry`]
// This is NOT thread safe, as udf_dirent_s internally holds
// the current position
pub struct UdfEntryReader<'a> {
    bytes_read: usize,
    entry: &'a UdfEntry<'a>,
}

impl Udf {
    /// Return the root entry of the filesystem.
    /// `None` is returned on error.
    pub fn root(&self) -> Option<UdfEntry<'_>> {
        // SAFETY: The returned value will be owned by UdfEntry
        let entry = unsafe { libcdio_sys::udf_get_root(self.udf.as_ptr(), true, 0) };

        Some(UdfEntry::new(NonNull::new(entry)?))
    }

    /// Return the root entry of the filesystem, from the given partition.
    /// `None` is returned on error.
    pub fn root_from_partition(&self, partition: u16) -> Option<UdfEntry<'_>> {
        let entry = unsafe { libcdio_sys::udf_get_root(self.udf.as_ptr(), false, partition) };

        Some(UdfEntry::new(NonNull::new(entry)?))
    }

    /// Return entry for `path`.
    ///
    /// Only '/' may be used for path separators.
    /// `None` is returned on error.
    pub fn entry(&self, path: &str) -> Option<UdfEntry<'_>> {
        let root = self.root()?;
        let path = CString::new(path).ok()?;
        let entry = unsafe { libcdio_sys::udf_fopen(root.entry.as_ptr(), path.as_ptr()) };

        Some(UdfEntry::new(NonNull::new(entry)?))
    }
}

impl UdfEntry<'_> {
    /// Return the modification time.
    /// Returns `None` in case the value is invalid.
    pub fn modify_time(&self) -> Option<OffsetDateTime> {
        // SAFETY: Returns -1 in case the value is invalid, checked immediately below
        let time = unsafe { libcdio_sys::udf_get_modification_time(self.entry.as_ptr()) };
        if time == -1 {
            return None;
        }

        OffsetDateTime::from_unix_timestamp(time).ok()
    }

    /// Return the filename.
    /// `None` is returned if the filename has non UTF-8 characters, or on an unexpected error.
    pub fn filename(&self) -> Option<&str> {
        const CURRENT_DIR_FILENAME: &str = ".";

        // SAFETY: self.entry is non null, therefore this method should not return null
        let filename = unsafe { libcdio_sys::udf_get_filename(self.entry.as_ptr()) };
        if filename.is_null() {
            tracing::error!("udf_get_filename() returned an unexpected NULL");
            return None;
        }
        let filename = unsafe { CStr::from_ptr(filename) };
        // filename returns an empty string after opening the root directory.
        // this probably represents "."
        if filename.is_empty() {
            return Some(CURRENT_DIR_FILENAME);
        }

        filename.to_str().ok()
    }

    /// Return the next entry, or `None` on reaching end of file or on error.
    pub fn next(self) -> Option<Self> {
        // SAFETY: This always frees the passed entry, therefore prevent self's destructor
        // from running
        let next_entry = unsafe { libcdio_sys::udf_readdir(self.entry.as_ptr()) };
        std::mem::forget(self);

        NonNull::new(next_entry).map(Self::new)
    }

    /// Open `self` and return the first entry.
    /// Returns `None` if `self` is not a directory, or on error.
    // TODO: Add unit test, need a UDF file with directory that works with libcdio
    pub fn open_dir(&self) -> Option<Self> {
        let sub_entry = unsafe { libcdio_sys::udf_opendir(self.entry.as_ptr()) };

        Some(Self::new(NonNull::new(sub_entry)?))
    }

    /// Is the entry a directory.
    pub fn is_dir(&self) -> bool {
        unsafe { libcdio_sys::udf_is_dir(self.entry.as_ptr()) }
    }

    /// Return the file length.
    pub fn file_length(&self) -> u64 {
        // SAFETY: entry is not null, making this function infallible
        unsafe { libcdio_sys::udf_get_file_length(self.entry.as_ptr()) }
    }

    /// Return the POSIX file mode.
    pub fn mode(&self) -> Mode {
        // `mode_t` is non-portable (16 or 32 bit)
        #[allow(clippy::useless_conversion)]
        let mode = u32::from(unsafe { libcdio_sys::udf_get_posix_filemode(self.entry.as_ptr()) });
        Mode::new(mode, u32::MAX)
    }

    /// Return the number of hard links of the entry.
    pub fn link_count(&self) -> u16 {
        unsafe { libcdio_sys::udf_get_link_count(self.entry.as_ptr()) }
    }

    /// Returns a type that implements [`io::Read`], to allow for reading the
    /// file entry corresponding to an [`Iso9660Stat`]
    /// Returns `None` on error.
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
            _phantom: PhantomData,
        }
    }
}

impl UdfEntryReader<'_> {
    /// Sets the internal position value that's stored the FFI boundary.
    // As of libcdio v2.3.0, the internal value i.e
    // udf_dirent_s.p_udf->i_position is only used by its provided read and
    // seek methods.
    // Therefore, prevent state leakage between independent instances of
    // Self in the corresponding rust methods by resetting the internal
    // position to zero in the corresponding rust methods.
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

impl Drop for UdfEntry<'_> {
    fn drop(&mut self) {
        // SAFETY: Infallible function
        let _ = unsafe { libcdio_sys::udf_dirent_free(self.entry.as_ptr()) };
    }
}

impl io::Read for UdfEntryReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let file_size = self.entry.file_length() as usize;
        let mut buf_read = 0;
        // prevent state leakage. refer method doc for more.
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
    use std::{io::Read, path::Path};

    use time::macros::datetime;

    use crate::udf::tests::test_udf_file;

    use super::*;

    fn test_udf_file1() -> &'static Path {
        Path::new("../test-data/udf1.iso")
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
        udf.entry("/licenses/COPYING.LESSER").unwrap();
    }
}
