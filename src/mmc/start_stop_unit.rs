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

//! Routines based on MMC `START STOP UNIT`.

use displaydoc::Display;
use thiserror::Error;

use crate::{
    Mmc,
    mmc::{Cdb, MmcCommand, MmcDirection, MmcError},
};

/// Routines based on MMC `START STOP UNIT`.
impl Mmc {
    /// Eject the media, if permitted.
    ///
    /// This might require a prior allow eject operation.
    pub fn eject(&self) -> Result<(), MmcEjectError> {
        self.start_stop_unit(StartStopOperation::EjectDisc)?;
        Ok(())
    }

    /// Close the tray.
    pub fn close_tray(&self) -> Result<(), MmcCloseTrayError> {
        self.start_stop_unit(StartStopOperation::LoadStartDisc)?;
        Ok(())
    }

    /// Set power state.
    pub fn set_power_state(&self, state: PowerCondition) -> Result<(), MmcSetPowerStateError> {
        self.start_stop_unit(StartStopOperation::Power(state))?;
        Ok(())
    }

    fn start_stop_unit(&self, operation: StartStopOperation) -> Result<(), MmcStartStopError> {
        let mut cdb = Cdb::default();

        cdb[0] = MmcCommand::StartStopUnit as u8;
        cdb[1] = 0; // not using the immediate bit for now
        if let StartStopOperation::Jump { layer_number } = operation {
            cdb[3] = layer_number & LAYER_NUM_BITMASK;
            cdb[4] |= 1 << FORMAT_LAYER_BITPOS;
        }
        // Sets the LoEj and Start fields
        // as described in 6.42.3.1 of MMC-6 2g
        cdb[4] |= match operation {
            StartStopOperation::StartDisc => 0b01,
            StartStopOperation::EjectDisc => 0b10,
            StartStopOperation::LoadStartDisc | StartStopOperation::Jump { .. } => 0b11,
            _ => 0b00,
        };
        if let StartStopOperation::Power(pow_cond) = operation {
            cdb[4] |= (pow_cond as u8 & POWER_COND_BITMASK) << POWER_COND_BITPOS;
        }

        self.run_command(Some(MmcDirection::Write), &mut [], cdb)?;

        return Ok(());

        const LAYER_NUM_BITMASK: u8 = 0b11;
        const FORMAT_LAYER_BITPOS: usize = 2;
        const POWER_COND_BITMASK: u8 = 0b1111;
        const POWER_COND_BITPOS: usize = 4;
    }
}

/// could not eject MMC device
#[derive(Debug, Display, Error)]
pub struct MmcEjectError {
    #[from]
    pub source: MmcStartStopError,
}

/// could not close tray of the MMC device
#[derive(Debug, Display, Error)]
pub struct MmcCloseTrayError {
    #[from]
    pub source: MmcStartStopError,
}

/// could not set power state of MMC device
#[derive(Debug, Display, Error)]
pub struct MmcSetPowerStateError {
    #[from]
    pub source: MmcStartStopError,
}

/// error from a `START STOP UNIT` command
#[derive(Debug, Display, Error)]
pub struct MmcStartStopError {
    #[from]
    pub source: MmcError,
}

/// Operations of the `START STOP UNIT` command
#[allow(unused)]
enum StartStopOperation {
    StopDisc,
    StartDisc,
    EjectDisc,
    LoadStartDisc,

    /// Change the online format-layer to the specified value for hybrid discs.
    /// Only the last two bits will be set.
    Jump {
        layer_number: u8,
    },

    /// Place the device in the specified power condition
    Power(PowerCondition),
}

/// A power state as defined under MMC `START STOP UNIT`
#[allow(unused)]
pub enum PowerCondition {
    Idle = 0x2,
    Standby = 0x3,
    Sleep = 0x5,
}

// TODO: add manual tests
