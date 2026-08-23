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

//! Routines based on MMC `PREVENT ALLOW MEDIUM REMOVAL`.

use displaydoc::Display;
use thiserror::Error;

use crate::{
    Mmc,
    mmc::{Cdb, MmcCommand, MmcDirection, MmcError},
};

/// Routines based on MMC `PREVENT ALLOW MEDIUM REMOVAL`
impl Mmc {
    /// Allow media removal.
    pub fn allow_media_removal(&self) -> Result<(), MmcMediaRemovalError> {
        self.prevent_allow_medium_removal(PreventOption::Clear)
    }

    /// Prevent media removal.
    pub fn prevent_media_removal(&self) -> Result<(), MmcMediaRemovalError> {
        self.prevent_allow_medium_removal(PreventOption::Set)
    }

    fn prevent_allow_medium_removal(
        &self,
        prevent: PreventOption,
    ) -> Result<(), MmcMediaRemovalError> {
        let mut cdb = Cdb::default();
        cdb[0] = MmcCommand::PreventAllowMediumRemoval as u8;
        cdb[4] = prevent as u8;

        self.run_command(Some(MmcDirection::Write), &mut [], cdb)?;

        Ok(())
    }
}

/// error from a `PREVENT ALLOW MEDIUM REMOVAL` command.
#[non_exhaustive]
#[derive(Debug, Display, Error)]
pub struct MmcMediaRemovalError {
    #[from]
    pub source: MmcError,
}

/// Operations of the `PREVENT ALLOW MEDIUM REMOVAL` command
#[allow(unused)]
#[repr(u8)]
#[derive(Clone, Copy, Debug)]
enum PreventOption {
    Clear = 0b00,
    Set = 0b01,
    ClearPersistent = 0b10,
    SetPersistent = 0b11,
}

// see tests/media_removal.rs
