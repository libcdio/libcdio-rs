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

//! Routines based on MMC `READ DISC INFORMATION`.

use displaydoc::Display;
use thiserror::Error;

use crate::{
    Mmc,
    mmc::{Cdb, MmcCommand, MmcDirection, MmcError},
};

/// Routines based on MMC `READ DISC INFORMATION`.
impl Mmc {
    /// Indicates that the media is of a writable kind (such as CD-RW, BD-RE,
    /// DVD+RW, etc) and the drive is capable of writing to the media.
    pub fn is_disc_erasable(&self) -> Result<bool, MmcReadDiscInfoError> {
        let data = self.read_disc_information(DiscInfoKind::Standard)?;
        Ok(data[2] & 1 << 4 != 0)
    }

    fn read_disc_information(&self, kind: DiscInfoKind) -> Result<Vec<u8>, MmcReadDiscInfoError> {
        let mut buf = vec![0; INITIAL_BUFFER_SIZE];
        let mut cdb = Cdb::default();
        cdb[0] = MmcCommand::ReadDiscInfo as u8;
        cdb[1] = kind as u8 & 0b111;
        cdb[7..9].copy_from_slice(&(buf.len() as u16).to_be_bytes());

        self.run_command(Some(MmcDirection::Read), buf.as_mut_slice(), cdb)?;

        let data_length = buf[0..2]
            .try_into()
            .map(u16::from_be_bytes)
            .map(|len| usize::from(len) + DATA_LEN_FIELDSIZE)
            .expect("initial buffer length is greater than two bytes");
        if buf.len() < data_length {
            buf.resize(data_length, 0);
            cdb[7..9].copy_from_slice(&(buf.len() as u16).to_be_bytes());
            self.run_command(Some(MmcDirection::Read), buf.as_mut_slice(), cdb)?;
        }
        buf.truncate(data_length);
        tracing::debug!(?buf, len = buf.len());

        return Ok(buf);

        const INITIAL_BUFFER_SIZE: usize = 64;
        const DATA_LEN_FIELDSIZE: usize = 2;
    }
}

#[allow(unused)]
#[repr(u8)]
#[derive(Clone, Copy, Debug)]
enum DiscInfoKind {
    /// Disc type, codes, sessions, opc entries..
    Standard = 0b000,

    /// Assigned, appendable and other track counts..
    TrackResources = 0b001,

    /// Pseudo Overwrite entries, updates and replacements
    PowResources = 0b010,
}

/// error from a `READ DISC INFORMATION` command.
#[derive(Debug, Display, Error)]
pub enum MmcReadDiscInfoError {
    /// operating system returned an error
    Os(#[from] MmcError),

    /// invalid response from mmc command: {0}
    InvalidResponse(String),
}

#[cfg(test)]
mod tests {
    use tracing::info;

    use super::*;

    #[test_log::test(test)]
    #[ignore = "requires a drive with mmc"]
    fn is_disc_erasable() {
        let is_disc_erasable = Mmc::new().unwrap().is_disc_erasable().unwrap();
        info!(is_disc_erasable);
    }
}
