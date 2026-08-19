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

//! Routines based on `READ CD`.
// See MMC-6 2g, 6.19 READ CD Command

use docsplay::Display;
use thiserror::Error;

use crate::{
    Mmc,
    mmc::{Cdb, MmcCommand, MmcDirection, MmcError},
};

/// Routines based on `READ CD`.
impl Mmc {
    /// Read disc using MMC `READ CD`.
    ///
    /// The transfer size is dependent on the opted fields in the read options.
    /// See [`ReadCdOptions`].
    pub fn read_cd(
        &self,
        options: ReadCdOptions,
        sector: u32,
        count: u16,
        buf: &mut [u8],
    ) -> Result<(), MmcReadCdError> {
        let mut cdb = Cdb::default();
        cdb[0] = MmcCommand::ReadCd as u8;
        if let Some(SectorOption::Cdda { dap: true }) = options.expected_sector_type {
            cdb[1] |= 1 << 1;
        }
        if let Some(sector_type) = options.expected_sector_type {
            cdb[1] |= (sector_type.discriminant() & 0b111) << 2;
        }
        cdb[2..=5].copy_from_slice(&sector.to_be_bytes());
        cdb[7..=8].copy_from_slice(&count.to_be_bytes());
        if options.include_sync {
            cdb[9] |= 1 << 7;
        }
        if options.include_subheader {
            cdb[9] |= 1 << 6;
        }
        if options.include_header {
            cdb[9] |= 1 << 5;
        }
        if options.include_user_data {
            cdb[9] |= 1 << 4;
        }
        if options.include_ecc {
            cdb[9] |= 1 << 3;
        }
        if let Some(c2_error) = options.include_c2_error {
            cdb[9] |= (c2_error as u8 & 0b11) << 1;
        }
        if let Some(subchan_option) = options.include_subchannel {
            cdb[10] |= subchan_option as u8 & 0b111;
        }

        self.run_command(Some(MmcDirection::Read), buf, cdb)?;

        Ok(())
    }
}

/// Disc read options.
///
/// A few options are valid only for certain sector types.
/// Non-contiguous combinations of fields may not be feasible to read, and will
/// result in an error (such as include_sync + include_User_data).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReadCdOptions {
    /// Restrict read to sector type.
    pub expected_sector_type: Option<SectorOption>,

    /// Include sync fields (12 bytes per sector).
    /// Sync fields are not present in CD-DA.
    pub include_sync: bool,

    /// Include headers (4 bytes per sector).
    /// Headers are present in all sector types except CD-DA.
    pub include_header: bool,

    /// Include sub-headers (8 bytes per sector).
    /// Sub-headers are present only in Mode2 formed sectors.
    pub include_subheader: bool,

    /// Include user data. Defaults to `true`.
    /// Size depends on the sector type of the disc.
    pub include_user_data: bool,

    /// Include ECC/EDC, i.e the fields that follow user data. Present only in
    /// Mode1 (288 bytes per sector) and Mode2 formed sectors
    /// (Form1: 280 bytes per sector; Form2: 4 bytes per sector).
    pub include_ecc: bool,

    /// Include "C2 error bits" (294 to 296 bytes per sector).
    pub include_c2_error: Option<C2Option>,

    /// Include sub-channel information. (16 to 96 bytes per sector).
    pub include_subchannel: Option<SubchannelOption>,
}

impl Default for ReadCdOptions {
    /// Include only user data.
    fn default() -> Self {
        Self {
            expected_sector_type: None,
            include_sync: false,
            include_header: false,
            include_subheader: false,
            include_user_data: true,
            include_ecc: false,
            include_c2_error: None,
            include_subchannel: None,
        }
    }
}

/// Sector type to include.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SectorOption {
    /// IEC 908 (CD-DA) with user data of 2352 bytes.
    Cdda {
        /// Return data with flaw obsuring mechanisms such as audio data mute
        /// and interpolate.
        dap: bool,
    } = 0b001,

    /// User data of 2048 bytes.
    #[default]
    Mode1 = 0b010,

    /// User data of 2336 bytes.
    Mode2Formless = 0b011,

    /// User data of 2048 bytes.
    Mode2Form1 = 0b100,

    /// User data of 2324 bytes.
    Mode2Form2 = 0b101,
}
impl SectorOption {
    fn discriminant(&self) -> u8 {
        // SAFETY: `repr(u8)` is laid out as `repr(C)` `union` with the `u8`
        // discriminant as its first field.
        // Source:
        // https://doc.rust-lang.org/stable/std/mem/fn.discriminant.html#accessing-the-numeric-value-of-the-discriminant
        unsafe { *(&raw const *self).cast::<u8>() }
    }
}

/// C2 error information to include.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum C2Option {
    /// 294 bytes of "C2 error bits".
    #[default]
    Raw = 0b01,

    /// 296 bytes consisting of:
    /// - Block error byte: logical OR of all the 294 bytes of "C2 error bits"
    /// - A pad byte of zero
    /// - 294 bytes of "C2 error bits"
    RawAndBlock = 0b10,
}

/// Sub-Channel information to include.
// This corresponds to the 'Sub-channel Selection Field Values'.
// Variant `0b000`, i.e 'no sub-channel data' can be represented using `None`.
// Variant `0b001`, i.e 'RawPw' has been marked as legacy since `MMC-6`.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SubchannelOption {
    /// RAW P-W sub-channel data (96 bytes)
    RawPw = 0b001,

    /// Formatted Q sub-channel data (16 bytes).
    #[default]
    Q = 0b010,

    /// Corrected and de-interleaved R-W sub-channel data (96 bytes).
    Rw = 0b100,
}

/// error from a `READ CD` command
#[derive(Debug, Display, Error)]
pub struct MmcReadCdError {
    #[from]
    pub source: MmcError,
}

#[cfg(test)]
mod tests {
    use tracing::info;

    use super::*;

    #[test_log::test(test)]
    #[ignore]
    fn read_cd() {
        let mmc = Mmc::new().unwrap();
        const SIZE: usize = 23520;
        let mut buf = vec![0; SIZE];
        mmc.read_cd(ReadCdOptions::default(), 200, 10, &mut buf)
            .unwrap();
        info!(
            "read buffer is populated about {:?} bytes out of {SIZE} bytes",
            buf.iter().rposition(|z| *z != 0),
        );
        assert!(buf.iter().any(|e| *e != 0));
    }
}
