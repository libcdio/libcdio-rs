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

//! # libcdio-rs
//! Safe wrapper and Rust port of [GNU libcdio][libcdio-site].
//!
//! ## SCSI MMC references
//! This library implements SCSI routines primarily based on
//! [MMC-6 rev.2g][mmc6r2g] and [SPC-3 rev.23][spc3r23].
//!
//! ## Changelog
//! See [CHANGELOG.md][changelog]
//!
//! [libcdio-site]: https://libcdio.github.io
//! [mmc6r2g]: https://www.13thmonkey.org/documentation/SCSI/mmc6r02g.pdf
//! [spc3r23]: https://www.13thmonkey.org/documentation/SCSI/spc3r23.pdf
//! [changelog]: https://github.com/libcdio/libcdio-rs/tree/main/CHANGELOG.md

#![cfg_attr(docsrs, feature(doc_cfg))]

mod cdio;
pub mod drive;

#[cfg(feature = "iso9660")]
pub mod iso9660;

mod logging;

#[cfg(feature = "mmc")]
pub mod mmc;

#[cfg(feature = "udf")]
pub mod udf;

#[doc(inline)]
pub use crate::drive::Drive;

#[cfg(feature = "iso9660")]
#[doc(inline)]
pub use crate::iso9660::Iso;

#[cfg(feature = "udf")]
#[doc(inline)]
pub use crate::udf::Udf;

#[cfg(feature = "mmc")]
#[doc(inline)]
pub use mmc::Mmc;

#[cfg(any(feature = "iso9660", feature = "udf"))]
pub use {file_mode, time};
