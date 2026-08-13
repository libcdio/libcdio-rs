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

//! ISO 9660 file/directory entry object.

use std::{
    ffi::{CStr, CString},
    io,
    path::Path,
    ptr::NonNull,
};

use libcdio_sys::{iso9660_stat_s, iso9660_stat_s__STAT_DIR};
use time::OffsetDateTime;

use crate::iso9660::{Iso9660, ds, util};

/// ISO 9660 file/directory entry.
pub struct Iso9660Entry<'a> {
    /// The parent ISO 9660 object
    pub(crate) iso: &'a Iso9660,
    pub(crate) stat: NonNull<iso9660_stat_s>,
}

/// A type that implements [`io::Read`], for reading an ISO9660 entry.
pub struct Iso9660EntryReader<'a> {
    bytes_read: usize,
    entry: &'a Iso9660Entry<'a>,
}

impl Iso9660 {
    /// Read directory at `path` and return a list of entries.
    /// Returns `None` on error.
    pub fn read_dir(&self, path: &Path) -> Option<Vec<Iso9660Entry<'_>>> {
        let path = CString::new(path.to_str()?).ok()?;
        let dirlist = unsafe { libcdio_sys::iso9660_ifs_readdir(self.ptr.as_ptr(), path.as_ptr()) };
        if dirlist.is_null() {
            return None;
        }
        // SAFETY: dirlist is not null and the data will be owned by `Iso9660Entry`.
        let dirlist = unsafe { ds::cdiolist_to_vec(dirlist) };
        let dirlist = dirlist
            .into_iter()
            .filter_map(|entry| {
                Some(Iso9660Entry {
                    iso: self,
                    stat: NonNull::new(entry.cast())?,
                })
            })
            .collect();

        Some(dirlist)
    }

    /// Return entry for `path`. `None` is returned on error.
    pub fn entry(&self, path: &Path) -> Option<Iso9660Entry<'_>> {
        let path = CString::new(path.to_str()?).ok()?;
        let stat = unsafe { libcdio_sys::iso9660_ifs_stat(self.ptr.as_ptr(), path.as_ptr()) };

        Some(Iso9660Entry {
            iso: self,
            stat: NonNull::new(stat)?,
        })
    }
}

impl Iso9660Entry<'_> {
    /// Returns the raw filename of the entry.
    /// Returns `None` if the filename has non UTF-8 characters or on error.
    pub fn filename_raw(&self) -> Option<&str> {
        // SAFETY: self.entry is not null since its behind a NonNull<T>
        let name = unsafe { (*self.stat.as_ptr()).filename.as_ptr() };
        if name.is_null() {
            return None;
        };
        // SAFETY: The filename should be a null terminated string
        let name = unsafe { CStr::from_ptr(name).to_str() };

        name.inspect_err(|err| tracing::error!(%err)).ok()
    }

    /// Returns a filename in a format used for a listing.
    /// - Lowercase name if no Joliet Extension interpretation.
    /// - Remove trailing ;1 or .;1
    /// - Turn the other ; into version numbers.
    ///
    /// Returns `None` if the string has non UTF-8 characters or on error.
    pub fn filename(&self) -> Option<String> {
        let filename = unsafe { (*self.stat.as_ptr()).filename.as_ptr() };
        if filename.is_null() {
            return None;
        }

        let filename = unsafe { CStr::from_ptr(filename) };
        let mut translated_name = vec![0; filename.count_bytes() + 1];
        let joliet_level = self.iso.joliet_level().map(u8::from).unwrap_or(0);

        let len = unsafe {
            libcdio_sys::iso9660_name_translate_ext(
                filename.as_ptr(),
                translated_name.as_mut_ptr().cast(),
                joliet_level,
            )
        };
        translated_name.truncate(len as usize);

        String::from_utf8(translated_name).ok()
    }

    /// Multi-extent aware size, in bytes.
    pub fn total_size(&self) -> u64 {
        unsafe { (*self.stat.as_ptr()).total_size }
    }

    /// Return the logical sector number.
    pub fn lsn(&self) -> i32 {
        unsafe { (*self.stat.as_ptr()).lsn }
    }

    /// Returns `true` if self is a directory.
    pub fn is_dir(&self) -> bool {
        unsafe { (*self.stat.as_ptr()).type_ == iso9660_stat_s__STAT_DIR }
    }

    /// Returns the timestamp on the entry.
    /// `None` if the timestamp is invalid.
    pub fn timestamp(&self) -> Option<OffsetDateTime> {
        let tm = unsafe { (*self.stat.as_ptr()).tm };
        util::convert_tm_local(tm).ok()
    }

    /// A type that implements [`io::Read`], for reading an ISO9660 entry.
    /// Returns `None` on error.
    pub fn reader(&self) -> Iso9660EntryReader<'_> {
        Iso9660EntryReader {
            bytes_read: 0,
            entry: self,
        }
    }
}

impl Drop for Iso9660Entry<'_> {
    fn drop(&mut self) {
        unsafe { libcdio_sys::iso9660_stat_free(self.stat.as_ptr()) }
    }
}

impl io::Read for Iso9660EntryReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let file_size = self.entry.total_size() as usize;
        let mut buf_read = 0;
        while self.bytes_read < file_size && buf_read < buf.len() {
            let lsn = self.entry.lsn() + (self.bytes_read / Iso9660::BLOCK_SIZE) as i32;
            let mut block = [0_u8; Iso9660::BLOCK_SIZE];
            let ret = unsafe {
                libcdio_sys::iso9660_iso_seek_read(
                    self.entry.iso.ptr.as_ptr(),
                    block.as_mut_ptr().cast(),
                    lsn,
                    1,
                )
            };
            // the returned value is either BLOCK_SIZE or zero on error, thus
            // excess bytes past the last read must be handled.
            // cast is safe as Iso9660::BLOCK_SIZE < i16::MAX
            if ret != block.len() as _ {
                return Err(io::Error::other(format!(
                    "error reading block at lsn: {lsn}",
                )));
            }

            // offset start to skip the first bytes given out during
            // a previous partial read() call
            let block_start = self.bytes_read % block.len();
            let buf_rem = buf.len() - buf_read;
            // skip out the excess bytes past file_size using .min()
            let block_rem = (block.len() - block_start).min(file_size - self.bytes_read);
            let len = buf_rem.min(block_rem);
            buf[buf_read..buf_read + len].copy_from_slice(&block[block_start..block_start + len]);
            buf_read += len;
            self.bytes_read += len;
        }

        Ok(buf_read)
    }
}

impl io::Seek for Iso9660EntryReader<'_> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.bytes_read = match pos {
            io::SeekFrom::Start(offset) => offset as usize,
            io::SeekFrom::End(offset) => {
                self.entry.total_size().saturating_add_signed(offset) as usize
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

    use crate::iso9660::{
        Iso9660,
        tests::{test_joliet_file, test_rockridge_file},
    };

    #[test]
    fn read_dir() {
        let iso = Iso9660::new(test_joliet_file()).unwrap();
        let entries = iso.read_dir(Path::new("/")).unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn filename() {
        let iso = Iso9660::new(test_rockridge_file()).unwrap();
        let entries = iso.read_dir(Path::new("/")).unwrap();
        let names: Vec<_> = entries.iter().map(|e| e.filename_raw().unwrap()).collect();
        assert_eq!(
            &names,
            &[".", "..", "copy", "Copy2", "COPYING", "fd0", "tmp", "zero"]
        );
    }

    #[test]
    fn filename_translated() {
        let iso = Iso9660::new(test_rockridge_file()).unwrap();
        let entries = iso.read_dir(Path::new("/")).unwrap();
        let names: Vec<_> = entries.iter().map(|e| e.filename().unwrap()).collect();
        assert_eq!(
            &names,
            &[".", "..", "copy", "copy2", "copying", "fd0", "tmp", "zero"]
        );
    }

    #[test]
    fn entry() {
        let iso = Iso9660::new(test_rockridge_file()).unwrap();
        let entry = iso.entry(Path::new("/copy")).unwrap();
        assert_eq!(entry.filename().unwrap(), "copy");
    }

    #[test]
    fn total_size() {
        let iso = Iso9660::new(test_rockridge_file()).unwrap();
        let entry = iso.entry(Path::new("/COPYING")).unwrap();
        assert_eq!(entry.total_size(), 17992);
    }

    #[test]
    fn lsn() {
        let iso = Iso9660::new(test_rockridge_file()).unwrap();
        let entry = iso.entry(Path::new("/COPYING")).unwrap();
        assert_eq!(entry.lsn(), 27);
    }

    #[test]
    fn is_dir() {
        let iso = Iso9660::new(test_rockridge_file()).unwrap();
        let file = iso.entry(Path::new("/COPYING")).unwrap();
        assert!(!file.is_dir());

        let dir = iso.entry(Path::new("/copy")).unwrap();
        assert!(dir.is_dir());
    }

    #[test]
    fn timestamp() {
        let iso = Iso9660::new(test_rockridge_file()).unwrap();
        let entry = iso.entry(Path::new("/COPYING")).unwrap();
        assert_eq!(
            entry.timestamp().unwrap(),
            datetime!(2005-03-05 20:55:51.0 +05:30:00),
        );
    }

    #[test]
    fn read() {
        let iso = Iso9660::new(Path::new("../test-data/xa.iso")).unwrap();
        let entry = iso.entry(Path::new("copying")).unwrap();
        let gpl = std::fs::read_to_string("../COPYING").unwrap();
        let mut reader = entry.reader();

        let mut result = String::new();
        let retval = reader.read_to_string(&mut result).unwrap();
        assert_eq!(gpl.len(), retval);
        assert_eq!(gpl, result);
    }
}
