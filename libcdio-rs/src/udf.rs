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

//! UDF filesystem.

mod entry;

pub use entry::UdfEntry;

use std::{ffi::CString, path::PathBuf, ptr::NonNull};

use libcdio_sys::udf_t;

use crate::logging;

/// UDF filesystem.
pub struct Udf {
    pub(crate) udf: NonNull<udf_t>,
}

impl Udf {
    pub const BLOCK_SIZE: usize = 2048;

    /// Open a UDF file. `None` is returned on error.
    pub fn new(path: PathBuf) -> Option<Self> {
        logging::init_logger();

        let path = CString::new(path.into_os_string().as_encoded_bytes()).ok()?;
        // SAFETY: The returned udf object is owned by Self and freed during drop
        let udf = unsafe { libcdio_sys::udf_open(path.as_ptr()) };

        Some(Self {
            udf: NonNull::new(udf)?,
        })
    }
}

impl Drop for Udf {
    fn drop(&mut self) {
        let _ = unsafe { libcdio_sys::udf_close(self.udf.as_mut()) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn test_udf_file() -> PathBuf {
        PathBuf::from("../test-data/udf.iso")
    }

    #[test]
    fn new() {
        let _ = Udf::new(test_udf_file()).unwrap();
    }
}
