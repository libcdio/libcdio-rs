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

//! Routines based on MMC `SET CD SPEED`.

use displaydoc::Display;
use thiserror::Error;

use crate::{
    Mmc,
    mmc::{Cdb, MmcCommand, MmcDirection, MmcError},
};

/// Routines based on MMC `SET CD SPEED`.
impl Mmc {
    /// Set the read and write speeds of the drive, in kilo bytes per second.
    pub fn set_cd_speed(
        &self,
        rotation_mode: RotationMode,
        read_speed: u16,
        write_speed: u16,
    ) -> Result<(), MmcSetCdSpeedError> {
        let mut cdb = Cdb::default();
        cdb[0] = MmcCommand::SetCdSpeed as u8;
        cdb[1] = rotation_mode as u8;
        cdb[2..4].copy_from_slice(&read_speed.to_be_bytes());
        cdb[4..6].copy_from_slice(&write_speed.to_be_bytes());

        self.run_command(Some(MmcDirection::Write), &mut [], cdb)?;

        Ok(())
    }
}

/// Rotation mode used by the drive.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RotationMode {
    /// Constant Angular Velocity: Maintains a fixed rotation rate.
    /// Provides faster seek times.
    #[default]
    Cav = 0b00,

    /// Constant Linear Velocity: Variable rotation rate.
    /// Provides consistent data rates.
    Clv = 0b01,
}

/// error from a `SET CD SPEED` command.
#[derive(Debug, Display, Error)]
pub struct MmcSetCdSpeedError {
    #[from]
    pub source: MmcError,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test(test)]
    #[ignore = "requires a drive with mmc"]
    fn set_cd_speed() {
        Mmc::new()
            .unwrap()
            .set_cd_speed(RotationMode::Cav, 0xffff, 0xffff)
            .unwrap();
    }
}
