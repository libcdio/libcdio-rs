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

#![cfg_attr(docsrs, feature(doc_cfg))]

mod cdio;
pub mod drive;

#[cfg(feature = "iso9660")]
pub mod iso9660;

mod logging;
pub mod mmc;

#[cfg(feature = "udf")]
pub mod udf;

#[doc(inline)]
pub use crate::{drive::Drive, mmc::Mmc};

#[cfg(feature = "iso9660")]
#[doc(inline)]
pub use crate::iso9660::Iso;

#[cfg(feature = "udf")]
#[doc(inline)]
pub use crate::udf::Udf;

#[cfg(any(feature = "iso9660", feature = "udf"))]
pub use {file_mode, time};
