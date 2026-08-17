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

//! Utility and data structure conversion routines.

use std::{error::Error, ffi::c_void};

use libcdio_sys::_CdioList;
use time::{Date, OffsetDateTime, Time, UtcOffset};

/// Convert `tm` representing local time to a `OffsetDateTime`.
pub(crate) fn convert_tm_local(
    tm: libcdio_sys::tm,
) -> Result<OffsetDateTime, Box<dyn Error + Send + Sync>> {
    const TM_YEAR_OFFSET: i32 = 1900;
    const TM_ORDINAL_DAY_OFFSET: u16 = 1;
    let date = Date::from_ordinal_date(
        tm.tm_year + TM_YEAR_OFFSET,
        u16::try_from(tm.tm_yday)? + TM_ORDINAL_DAY_OFFSET,
    )?;
    let time = Time::from_hms(
        u8::try_from(tm.tm_hour)?,
        u8::try_from(tm.tm_min)?,
        u8::try_from(tm.tm_sec)?,
    )?;

    Ok(OffsetDateTime::new_in_offset(
        date,
        time,
        UtcOffset::local_offset_at(OffsetDateTime::new_utc(date, time))?,
    ))
}

/// Returns a vec of pointers to the data of the cdio list.
/// Frees the list nodes, without freeing the data.
/// # Safety
/// - `cdio_list` must not be null.
/// - The list data must be owned by the caller.
pub unsafe fn cdiolist_to_vec(cdio_list: *mut _CdioList) -> Vec<*mut c_void> {
    let mut list = Vec::new();
    let mut cur = unsafe { libcdio_sys::_cdio_list_begin(cdio_list) };
    while !cur.is_null() {
        let data = unsafe { libcdio_sys::_cdio_list_node_data(cur) };
        list.push(data);
        cur = unsafe { libcdio_sys::_cdio_list_node_next(cur) };
    }

    unsafe {
        libcdio_sys::_cdio_list_free(cdio_list, 0, None);
    }

    list
}

#[cfg(test)]
mod tests {
    use std::ffi::CString;

    #[test]
    fn cdiolist_to_vec() {
        let a = CString::new("This is A").unwrap();
        let b = CString::new("This is B").unwrap();

        let cdiolist = unsafe { libcdio_sys::_cdio_list_new() };
        unsafe { libcdio_sys::_cdio_list_append(cdiolist, a.into_raw().cast()) };
        unsafe { libcdio_sys::_cdio_list_append(cdiolist, b.into_raw().cast()) };

        let list = unsafe { super::cdiolist_to_vec(cdiolist) };
        let a = unsafe { CString::from_raw(list[0].cast()) };
        let b = unsafe { CString::from_raw(list[1].cast()) };

        assert_eq!(&a, c"This is A");
        assert_eq!(&b, c"This is B");
    }
}
