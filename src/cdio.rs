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

use std::{
    ffi::{CStr, c_char},
    ops::Deref,
    ptr::{self, NonNull},
    sync::Mutex,
};

use libcdio_sys::{CdIo_t, driver_id_t, driver_id_t_DRIVER_DEVICE};

use crate::logging;

/// The Cdio type.
pub(crate) struct Cdio {
    pub(crate) cdio: NonNull<CdIo_t>,
}

impl Cdio {
    /// Initialize a hardware Cdio resource with read-write access.
    pub(crate) fn with_device(device: Option<&CStr>) -> Option<Self> {
        let source = device.map(|s| s.as_ptr()).unwrap_or(ptr::null());
        NonNull::new(Self::open(true, source, driver_id_t_DRIVER_DEVICE)).map(|cdio| Self { cdio })
    }

    fn open(allow_writes: bool, source: *const c_char, driver: driver_id_t) -> *mut CdIo_t {
        logging::init_logger();
        let access_mode = if allow_writes {
            RW_ACCESS_MODE.as_ptr()
        } else {
            ptr::null()
        };

        // SAFETY: This invokes cdio_init(), which mutates a static variable.
        // CDIO_LAST_DRIVER_LOCK is held to prevent data races.
        let _lock = CDIO_LAST_DRIVER_LOCK.lock().unwrap();
        return unsafe { libcdio_sys::cdio_open_am(source, driver, access_mode) };

        /// Although prefixed "MMC", this does imply read-write for all
        /// operations
        static RW_ACCESS_MODE: &CStr = c"MMC_RDWR";
    }
}

impl Deref for Cdio {
    type Target = NonNull<CdIo_t>;

    fn deref(&self) -> &Self::Target {
        &self.cdio
    }
}

impl Drop for Cdio {
    fn drop(&mut self) {
        let _lock = CDIO_LAST_DRIVER_LOCK.lock().unwrap();

        // SAFETY: This method invokes modifies a static variable.
        // CDIO_LAST_DRIVER_LOCK is held to prevent data races.
        unsafe { libcdio_sys::cdio_destroy(self.cdio.as_ptr()) }
    }
}

/// A lock guarding a private static named `CdIo_last_driver`. It must be held
/// before invoking any libcdio methods that modify this value.
/// As of libcdio v2.3.0, such methods are `cdio_init()` and `cdio_destroy()`.
static CDIO_LAST_DRIVER_LOCK: Mutex<()> = Mutex::new(());
