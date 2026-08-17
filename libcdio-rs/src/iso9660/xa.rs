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

//! CD-ROM XA (eXtended Architecture)

use bitflags::bitflags;

use crate::iso9660::entry::IsoEntry;

impl IsoEntry<'_> {
    /// Return CD-ROM XA (eXtended Architecture) attributes.
    /// `None` is returned if the attributes are not present.
    pub fn xa(&self) -> Option<CdRomXa> {
        let have_xa = unsafe { (*self.stat.as_ptr()).b_xa };
        if !have_xa {
            return None;
        }

        // SAFETY: The above check confirms that xa are present.
        let xa = unsafe { (*self.stat.as_ptr()).xa };

        Some(CdRomXa {
            file_attr: XaFileAttributes::from_bits_retain(u16::from_be(xa.attributes)),
            file_num: u8::from_be(xa.filenum),
            group_id: u16::from_be(xa.group_id),
            user_id: u16::from_be(xa.user_id),
            total_size: self.total_size(),
        })
    }
}

/// CD-ROM XA (eXtended Architecture) attributes
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct CdRomXa {
    pub file_attr: XaFileAttributes,
    pub file_num: u8,
    pub group_id: u16,
    pub user_id: u16,
    total_size: u64,
}

impl CdRomXa {
    /// Return multi extent size.
    /// Returns `None` if not using Mode2/Form2 encoding.
    // TODO: Add unit test
    pub const fn mode2form2_size(&self) -> Option<u64> {
        if !self.file_attr.contains(XaFileAttributes::Mode2Form2) {
            return None;
        }

        const ISO_BLOCK_BYTES: u64 = 2048;
        const MODE2FORM2_SECTOR_BYTES: u64 = 2324;

        let total_sectors = self.total_size.div_ceil(ISO_BLOCK_BYTES);

        Some(total_sectors * MODE2FORM2_SECTOR_BYTES)
    }
}

bitflags! {
    /// XA File Attributes.
    /// For more information: https://psx-spx.consoledev.net/cdromformat/#cdrom-iso-file-and-directory-descriptors
    #[derive(Clone, Copy, Debug)]
    pub struct XaFileAttributes: u16 {
        const OwnerRead = 1 << 0;
        const OwnerExecute = 1 << 2;
        const GroupRead = 1 << 4;
        const GroupExecute = 1 << 6;
        const WorldRead = 1 << 8;
        const WorldExecute = 1 << 10;
        const Mode2 = 1 << 11;
        const Mode2Form2 = 1 << 12;
        const Interleaved = 1 << 13;
        const Cdda = 1 << 14;
        const Directory = 1 << 15;
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::iso9660::Iso;

    use super::*;

    #[test]
    fn xa() {
        let iso = Iso::new(PathBuf::from("../test-data/xa.iso")).unwrap();
        let entry = iso.entry("/copying").unwrap();
        let xa = entry.xa().unwrap();
        assert_eq!(xa.file_num, 0);
        assert_eq!(xa.group_id, 3000);
        assert_eq!(xa.user_id, 1000);

        let expected_attr =
            XaFileAttributes::GroupRead & XaFileAttributes::GroupExecute & XaFileAttributes::Mode2;
        assert!(xa.file_attr.contains(expected_attr));
    }
}
