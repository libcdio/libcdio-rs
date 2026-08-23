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
    sync::Once,
};

use libcdio_sys::{
    cdio_log_level_t, cdio_log_level_t_CDIO_LOG_ASSERT, cdio_log_level_t_CDIO_LOG_DEBUG,
    cdio_log_level_t_CDIO_LOG_ERROR, cdio_log_level_t_CDIO_LOG_INFO,
    cdio_log_level_t_CDIO_LOG_WARN,
};

/// Configures libcdio to emit tracing (or "log" crate) logs.
/// This should be called before using any other methods.
pub(crate) fn init_logger() {
    LOG_HANDLER.call_once(|| unsafe {
        libcdio_sys::cdio_log_set_handler(Some(log_handler));

        // libcdio doesn't call our log handler unless its log level is
        // appropriate. Since the current log level is chosen by log
        // subscribers, hack around by setting libcdio's log level to debug.
        libcdio_sys::cdio_loglevel_default = cdio_log_level_t_CDIO_LOG_DEBUG;
    });
}

static LOG_HANDLER: Once = Once::new();

/// Handle logs via tracing.
extern "C" fn log_handler(level: cdio_log_level_t, message: *const c_char) {
    if message.is_null() {
        return;
    }
    // SAFETY: message is not null, and is backed by an array on the
    // caller's stack.
    let message = unsafe { CStr::from_ptr(message) };
    let Ok(message) = message.to_str() else {
        return;
    };
    #[allow(non_upper_case_globals)]
    match level {
        cdio_log_level_t_CDIO_LOG_ASSERT | cdio_log_level_t_CDIO_LOG_ERROR => {
            tracing::error!(message)
        }
        cdio_log_level_t_CDIO_LOG_WARN => tracing::warn!(message),
        cdio_log_level_t_CDIO_LOG_INFO => tracing::info!(message),
        _ => tracing::debug!(message),
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test(test)]
    fn log_test() {
        init_logger();
        unsafe {
            libcdio_sys::cdio_log(cdio_log_level_t_CDIO_LOG_ASSERT, c"assert msg".as_ptr());
            libcdio_sys::cdio_log(cdio_log_level_t_CDIO_LOG_ERROR, c"error msg".as_ptr());
            libcdio_sys::cdio_log(cdio_log_level_t_CDIO_LOG_WARN, c"warn msg".as_ptr());
            libcdio_sys::cdio_log(cdio_log_level_t_CDIO_LOG_INFO, c"info msg".as_ptr());
            libcdio_sys::cdio_log(cdio_log_level_t_CDIO_LOG_DEBUG, c"debug msg".as_ptr());
        }
    }
}
