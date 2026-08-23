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

//! Routines on MMC `TEST UNIT READY`.

use displaydoc::Display;
use thiserror::Error;

use crate::{
    Mmc,
    mmc::{Cdb, MmcCommand, MmcError},
};

/// Routines on MMC `TEST UNIT READY`.
impl Mmc {
    /// Returns `Ok` if the device is ready.
    pub fn test_unit_ready(&self) -> Result<(), MmcTestUnitReadyError> {
        let mut cdb = Cdb::default();
        cdb[0] = MmcCommand::TestUnitReady as u8;

        self.run_command(None, &mut [], cdb)?;

        Ok(())
    }
}

/// error from an MMC `TEST UNIT READY` command.
#[derive(Debug, Display, Error)]
pub struct MmcTestUnitReadyError {
    #[from]
    pub source: MmcError,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test(test)]
    #[ignore = "requires a drive with mmc"]
    fn test_unit_ready() {
        Mmc::new().unwrap().test_unit_ready().unwrap();
    }
}
