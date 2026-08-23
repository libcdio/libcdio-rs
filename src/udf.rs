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

pub use entry::*;

mod entry;

use thiserror::Error;

use std::{
    error::Error,
    ffi::{CString, OsString},
    path::PathBuf,
    ptr::NonNull,
};

use libcdio_sys::udf_t;

use crate::logging;

/// A UDF filesystem instance.
pub struct Udf {
    pub(crate) udf: NonNull<udf_t>,
}

impl Udf {
    /// The number of bytes in a UDF block.
    pub const BLOCK_SIZE: usize = 2048;

    /// Opens a UDF filesystem at `path`.
    pub fn new(path: PathBuf) -> Result<Self, UdfOpenError> {
        logging::init_logger();

        let path = CString::new(path.into_os_string().as_encoded_bytes())
            .map_err(|err| UdfOpenError::new(err.clone().into_vec(), Some(err.into())))?;
        let udf = unsafe { libcdio_sys::udf_open(path.as_ptr()) };

        NonNull::new(udf)
            .map(|udf| Self { udf })
            .ok_or_else(|| UdfOpenError::new(path.into_bytes(), None))
    }
}

impl Drop for Udf {
    fn drop(&mut self) {
        let _ = unsafe { libcdio_sys::udf_close(self.udf.as_mut()) };
    }
}

#[derive(Debug, Error)]
#[error(transparent)]
pub struct UdfOpenError(Box<Repr>);

#[derive(Debug, Error)]
#[error("error opening UDF filesystem at `{:?}`", path)]
struct Repr {
    path: PathBuf,
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl UdfOpenError {
    /// The path used to open the UDF file.
    pub fn path(self) -> PathBuf {
        self.0.path
    }

    fn new(path_bytes: Vec<u8>, source: Option<Box<dyn Error + Send + Sync>>) -> Self {
        Self(Box::new(Repr {
            // SAFETY: path_bytes originate from a `PathBuf`
            path: unsafe { OsString::from_encoded_bytes_unchecked(path_bytes) }.into(),
            source,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn test_udf_file() -> PathBuf {
        PathBuf::from("tests/data/udf.iso")
    }

    #[test]
    fn new() {
        let _ = Udf::new(test_udf_file()).unwrap();
    }
}
