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

//! Utility methods such as conversions

use time::{Date, OffsetDateTime, Time, UtcOffset, error};

/// Convert `tm` representing local time to a `OffsetDateTime`.
pub(crate) fn convert_tm_local(
    tm: libcdio_sys::tm,
) -> Result<OffsetDateTime, error::ComponentRange> {
    const TM_YEAR_OFFSET: i32 = 1900;
    const TM_ORDINAL_DAY_OFFSET: u16 = 1;
    let date = Date::from_ordinal_date(
        tm.tm_year + TM_YEAR_OFFSET,
        tm.tm_yday as u16 + TM_ORDINAL_DAY_OFFSET,
    )?;
    let time = Time::from_hms(tm.tm_hour as _, tm.tm_min as _, tm.tm_sec as _)?;

    Ok(OffsetDateTime::new_in_offset(
        date,
        time,
        UtcOffset::current_local_offset().expect("could not obtain the system offset"),
    ))
}
