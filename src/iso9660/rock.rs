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

//! Routines related to ISO 9660 Rock Ridge extensions.

use std::{ffi::CStr, mem::MaybeUninit};

use file_mode::Mode;
use libcdio_sys::{bool_3way_t_nope, bool_3way_t_yep, iso_rock_time_s};
use thiserror::Error;
use time::OffsetDateTime;

use crate::iso9660::{Iso, entry::IsoEntry, util};

impl Iso {
    /// Checks if any file has Rock Ridge extensions.
    ///
    /// This can be time consuming, therefore `file_limit` can be provided to
    /// limit the number of files to scan.
    pub fn have_rock_ridge(&self, file_limit: Option<u64>) -> Result<bool, RockRidgeSearchError> {
        let file_limit = file_limit.unwrap_or(u64::MAX);
        let result = unsafe { libcdio_sys::iso9660_have_rr(self.ptr.as_ptr(), file_limit) };

        #[allow(non_upper_case_globals)]
        match result {
            bool_3way_t_yep => Ok(true),
            bool_3way_t_nope => Ok(false),
            _ => Err(RockRidgeSearchError),
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Error)]
#[error("error searching for rock ridge extensions: file limit reached")]
pub struct RockRidgeSearchError;

impl IsoEntry<'_> {
    /// Returns the Rock Ridge attributes of the entry.
    pub fn rock_ridge(&self) -> Option<RockRidgeAttributes> {
        let rock = unsafe { (*self.stat.as_ptr()).rr };
        if rock.b3_rock != bool_3way_t_yep {
            return None;
        }

        Some(RockRidgeAttributes {
            create_time: convert_rock_timefield(rock.create),
            group_id: rock.st_gid,
            hard_links: rock.st_nlinks,
            mode: Mode::new(rock.st_mode, u32::MAX),
            modify_time: convert_rock_timefield(rock.modify),
            symlink_to: {
                if rock.psz_symlink.is_null() {
                    None
                } else {
                    let symlink = unsafe { CStr::from_ptr(rock.psz_symlink) };
                    symlink
                        .to_str()
                        .ok()
                        .filter(|link| !link.is_empty())
                        .map(ToString::to_string)
                }
            },
            user_id: rock.st_uid,
        })
    }
}

/// ISO 9660 Rock Ridge extensions.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct RockRidgeAttributes {
    pub create_time: Option<OffsetDateTime>,
    pub group_id: u32,
    pub hard_links: u32,
    pub mode: Mode,
    pub modify_time: Option<OffsetDateTime>,
    pub symlink_to: Option<String>,
    pub user_id: u32,
}

fn convert_rock_timefield(field: iso_rock_time_s) -> Option<OffsetDateTime> {
    if !field.b_used {
        return None;
    };

    let mut tm = MaybeUninit::uninit();
    if field.b_longdate {
        // SAFETY: ltime is valid as indicated by the if check
        unsafe { libcdio_sys::iso9660_get_ltime(&raw const field.t.ltime, tm.as_mut_ptr()) };
    } else {
        // SAFETY: dtime is valid as indicated by the if check
        unsafe { libcdio_sys::iso9660_get_dtime(&raw const field.t.dtime, true, tm.as_mut_ptr()) };
    }
    // SAFETY: The above ffi calls are infallible, thus tm should be initialized.
    let tm = unsafe { tm.assume_init() };

    util::convert_tm_local(tm).ok()
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use crate::iso9660::tests::{test_joliet_file, test_rockridge_file};

    use super::*;

    #[test]
    fn have_rock_ridge() {
        let iso = Iso::new(test_rockridge_file()).unwrap();
        assert!(iso.have_rock_ridge(None).unwrap());
    }

    #[test]
    fn rock_ridge() {
        let iso = Iso::new(test_rockridge_file()).unwrap();
        let entry = iso.entry("/COPYING".to_string()).unwrap();
        assert!(entry.rock_ridge().is_some());

        let iso = Iso::new(test_joliet_file()).unwrap();
        let entry = iso.entry("/libcdio/COPYING".to_string()).unwrap();
        assert!(entry.rock_ridge().is_none());
    }

    #[test]
    fn mode() {
        let iso = Iso::new(test_rockridge_file()).unwrap();

        let entry = iso.entry("/zero".to_string()).unwrap();
        let mode = entry.rock_ridge().unwrap().mode;
        assert_eq!(&mode.to_string(), "cr--r--r--");

        let entry = iso.entry("/fd0".to_string()).unwrap();
        let mode = entry.rock_ridge().unwrap().mode;
        assert_eq!(&mode.to_string(), "br--r--r--");

        let entry = iso.entry("/Copy2".to_string()).unwrap();
        let mode = entry.rock_ridge().unwrap().mode;
        assert_eq!(&mode.to_string(), "lr-xr-xr-x");

        let entry = iso.entry("/copy".to_string()).unwrap();
        let mode = entry.rock_ridge().unwrap().mode;
        assert_eq!(&mode.to_string(), "dr-xr-xr-x");
    }

    #[test]
    fn symlink_to() {
        let iso = Iso::new(test_rockridge_file()).unwrap();

        let entry = iso.entry("/COPYING".to_string()).unwrap();
        let rock = entry.rock_ridge().unwrap();
        assert!(rock.symlink_to.is_none());

        let entry = iso.entry("/Copy2".to_string()).unwrap();
        let rock = entry.rock_ridge().unwrap();
        assert_eq!(rock.symlink_to.unwrap(), "COPYING");

        let entry = iso.entry("/tmp/COPYING".to_string()).unwrap();
        let rock = entry.rock_ridge().unwrap();
        assert_eq!(rock.symlink_to.unwrap(), "../copying/COPYING");
    }

    #[test]
    fn hard_links() {
        let iso = Iso::new(test_rockridge_file()).unwrap();
        let entry = iso.entry("/COPYING".to_string()).unwrap();
        let rock = entry.rock_ridge().unwrap();
        assert_eq!(rock.hard_links, 1);

        let entry = iso.entry("/copy".to_string()).unwrap();
        let rock = entry.rock_ridge().unwrap();
        assert_eq!(rock.hard_links, 2);
    }

    #[test]
    fn user_id() {
        let iso = Iso::new(test_rockridge_file()).unwrap();
        let entry = iso.entry("/COPYING".to_string()).unwrap();
        let rock = entry.rock_ridge().unwrap();
        assert_eq!(rock.user_id, 0);
    }

    #[test]
    fn group_id() {
        let iso = Iso::new(test_rockridge_file()).unwrap();
        let entry = iso.entry("/COPYING".to_string()).unwrap();
        let rock = entry.rock_ridge().unwrap();
        assert_eq!(rock.group_id, 0);
    }

    #[test]
    fn time() {
        let iso = Iso::new(test_rockridge_file()).unwrap();
        let entry = iso.entry("/COPYING".to_string()).unwrap();
        let rock = entry.rock_ridge().unwrap();
        assert_eq!(
            rock.modify_time.unwrap(),
            datetime!(2005-03-05 20:55:51.0 +05:30:00)
        );
    }
}
