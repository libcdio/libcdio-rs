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

//! Routines based on MMC `READ TOC/PMA/ATIP`.

use displaydoc::Display;
use num_enum::{TryFromPrimitive, TryFromPrimitiveError};
use thiserror::Error;
use tracing::debug;
use winnow::{
    Parser,
    binary::{
        be_u16, be_u32,
        bits::{bits, take as bits_take},
        u8,
    },
    error::{ContextError, StrContext},
    token::take,
};

use crate::{
    Mmc,
    mmc::{Cdb, LEADOUT_TRACK, MmcCommand, MmcDirection, MmcError},
};

/// Routines based on MMC `READ TOC/PMA/ATIP`.
impl Mmc {
    /// Get the CD-TEXT from the RW sub-channel of the media.
    pub fn cd_text(&self) -> Result<Vec<u8>, MmcTocError> {
        let mut data = self.read_toc(ResponseFormat::Cdtext)?;
        let input = &mut data.as_slice();
        parse_header(input)?;

        let cdtext_len = input.len();
        // modify `data` itself to the result instead of using the header parsed
        // `input` to avoid an allocation
        data.drain(0..HEADER_SIZE);
        data.truncate(cdtext_len);

        return Ok(data);

        const HEADER_SIZE: usize = 4;
    }

    /// Get the CD type of the media.
    pub fn cd_type(&self) -> Result<CdType, MmcTocError> {
        let data = self.read_toc(ResponseFormat::FullToc {
            session_number: u8::default(),
        })?;
        let input = &mut data.as_slice();
        parse_header(input)?;

        let (
            _session_num,
            (_adr, _control),
            _tno,
            _point,
            _min,
            _sec,
            _frame,
            _zero,
            _pmin,
            psec,
            _pframe,
        ) = (
            u8, // session
            bits((
                bits_take::<_, u8, _, ContextError>(ADR_BITS),
                bits_take::<_, u8, _, _>(CONTROL_BITS),
            )),
            u8, // tno
            u8.verify(|point| *point == POINT),
            u8, // min
            u8, // sec
            u8, // frame
            u8.verify(|zero| *zero == 0),
            u8, // pmin
            u8, // psec
            u8, // pframe
        )
            .context(StrContext::Label("raw toc descriptor"))
            .parse_next(input)?;

        return CdType::try_from(psec).map_err(MmcTocError::from);

        const ADR_BITS: usize = 4;
        const CONTROL_BITS: usize = 4;
        const POINT: u8 = 0xA0; // with this value, PSEC will have disc type
    }

    /// Get the last Logical Sector Number (LSN) of the disc
    pub fn last_sector(&self) -> Result<u32, MmcTocError> {
        let data = self.read_toc(ResponseFormat::Toc {
            address_format: AddressFormat::Lba,
            track_number: LEADOUT_TRACK,
        })?;
        let input = &mut data.as_slice();
        parse_header(input)?;

        let (_, (_adr, _control), _track_num, _, track_start_address) = (
            u8, // reserved
            bits((
                bits_take::<_, u8, _, ContextError>(ADR_BITS), // adr
                bits_take::<_, u8, _, _>(CONTROL_BITS),        // control
            )),
            u8.verify(|track_num| *track_num == LEADOUT_TRACK),
            u8,     // reserved
            be_u32, // track start address
        )
            .context(StrContext::Label("formatted toc descriptor"))
            .parse_next(input)?;

        // This address represents the lead-out (ending) of the disc, reading
        // from the "track_start_address" returns an MMC out of range error,
        // thus the last (readable) sector is one less than this address.
        return Ok(track_start_address.saturating_sub(1));

        const ADR_BITS: usize = 4;
        const CONTROL_BITS: usize = 4;
    }

    fn read_toc(&self, format: ResponseFormat) -> Result<Vec<u8>, MmcTocError> {
        let mut buf = vec![0; DEFAULT_BUFFER_SIZE];
        let mut cdb = Cdb::default();

        cdb[0] = MmcCommand::ReadToc as u8;
        if let ResponseFormat::Toc { address_format, .. }
        | ResponseFormat::SessionInfo { address_format } = format
            && let AddressFormat::Time = address_format
        {
            cdb[1] |= 1 << TIME_BITPOS;
        }
        cdb[2] = format.discriminant();
        if let ResponseFormat::Toc {
            track_number: num, ..
        }
        | ResponseFormat::FullToc {
            session_number: num,
        } = format
        {
            cdb[6] = num;
        }
        cdb[7..9].copy_from_slice(&(buf.len() as u16).to_be_bytes());

        self.run_command(Some(MmcDirection::Read), buf.as_mut_slice(), cdb)?;

        let descriptor_length = buf[0..DTORLEN_SIZE]
            .try_into()
            .map(u16::from_be_bytes)
            .expect("buffer length is greater than two bytes");
        let data_size = usize::from(descriptor_length) + HEADER_SIZE; // extra 4 for the header
        if data_size > buf.len() {
            buf.resize(data_size, u8::default());
            cdb[7..9].copy_from_slice(&(buf.len() as u16).to_be_bytes());
            self.run_command(Some(MmcDirection::Read), buf.as_mut_slice(), cdb)?;
        }

        return Ok(buf);

        const DEFAULT_BUFFER_SIZE: usize = 64;
        const TIME_BITPOS: usize = 1;
        const DTORLEN_SIZE: usize = 2;
        const HEADER_SIZE: usize = 4;
    }
}

/// Parse the header and return the third and fourth byte fields
fn parse_header(input: &mut &[u8]) -> Result<(u8, u8), MmcTocError> {
    debug!(?input, len = input.len(), "parse_header()");

    let descriptors_length = be_u16::<_, ContextError>
        .context(StrContext::Label("READ TOC/PMA/ATIP header"))
        .parse_next(input)?;
    let (first_byte, second_byte, descriptors) =
        (u8::<_, ContextError>, u8, take(descriptors_length))
            .context(StrContext::Label("READ TOC/PMA/ATIP header"))
            .parse_next(input)?;
    *input = descriptors;

    Ok((first_byte, second_byte))
}

/// error from a `READ TOC/PMA/ATIP` command
#[non_exhaustive]
#[derive(Debug, Display, Error)]
pub enum MmcTocError {
    /// operating system returned an error
    Os(#[from] MmcError),

    /// invalid response from mmc command: {0}
    InvalidResponse(String),
}
impl From<ContextError> for MmcTocError {
    fn from(err: ContextError) -> Self {
        Self::InvalidResponse(err.to_string())
    }
}
impl<T: TryFromPrimitive> From<TryFromPrimitiveError<T>> for MmcTocError {
    fn from(err: TryFromPrimitiveError<T>) -> Self {
        Self::InvalidResponse(err.to_string())
    }
}

#[allow(unused)]
#[derive(Clone, Copy, Debug)]
enum AddressFormat {
    Lba,
    Time,
}

#[repr(u8)]
#[allow(unused)]
#[derive(Clone, Copy, Debug)]
enum ResponseFormat {
    Toc {
        address_format: AddressFormat,
        track_number: u8,
    } = 0b0000,
    SessionInfo {
        address_format: AddressFormat,
    } = 0b0001,
    FullToc {
        session_number: u8,
    } = 0b0010,
    Pma = 0b0011,
    Atip = 0b0100,
    Cdtext = 0b0101,
}
impl ResponseFormat {
    fn discriminant(&self) -> u8 {
        // SAFETY: Because `Self` is marked `repr(u8)`, its layout is a `repr(C)` `union`
        // between `repr(C)` structs, each of which has the `u8` discriminant as its first
        // field, so we can read the discriminant without offsetting the pointer.
        // Source:
        // https://doc.rust-lang.org/stable/std/mem/fn.discriminant.html#accessing-the-numeric-value-of-the-discriminant
        unsafe { *(&raw const *self).cast::<u8>() }
    }
}

/// The type of a CD disc
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, TryFromPrimitive)]
pub enum CdType {
    /// CD-DA or CD Data with first track in Mode 1
    #[default]
    CddaOrCdData = 0x00,

    /// CD-I
    Cdi = 0x10,

    /// CD-ROM XA disc with first track in Mode 2
    CdromXa = 0x20,
}

#[cfg(test)]
mod tests {
    use tracing::info;

    use super::*;

    #[test_log::test(test)]
    #[ignore = "requires a disc drive with mmc"]
    fn cd_text() {
        let cd_text = Mmc::new().unwrap().cd_text().unwrap();
        info!(?cd_text, len = cd_text.len());
        assert!(!cd_text.is_empty());
    }

    #[test_log::test(test)]
    #[ignore = "requires a disc drive with mmc"]
    fn cd_type() {
        let cd_type = Mmc::new().unwrap().cd_type().unwrap();
        info!(?cd_type);
    }

    #[test_log::test(test)]
    #[ignore = "requires a disc drive with mmc"]
    fn last_sector() {
        let last_sector = Mmc::new().unwrap().last_sector().unwrap();
        info!(?last_sector);
    }
}
