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

//! Routines based on MMC `READ SUB-CHANNEL`.

use displaydoc::Display;
use num_enum::{TryFromPrimitive, TryFromPrimitiveError};
use thiserror::Error;
use tracing::debug;
use winnow::{
    Parser,
    binary::{
        be_u16,
        bits::{bits, bool, take as bits_take},
        length_take, u8,
    },
    error::{ContextError, StrContext},
    token::take,
};

use crate::{
    Mmc,
    mmc::{Cdb, MmcDirection, MmcError},
};

/// Routines based on MMC `READ SUB-CHANNEL`.
impl Mmc {
    /// Get the status of audio play operations.
    pub fn audio_status(&self) -> Result<MmcAudioStatus, MmcAudioStatusError> {
        let data =
            self.read_subchannel(AddressFormat::Lba, SubchannelParameter::CdCurrentPosition)?;
        let status = parse_header(&mut data.as_slice())?;

        status.ok_or(MmcAudioStatusError::NotSupported)
    }

    /// Get the Media Catalog Number (UPC/bar code), if found.
    pub fn media_catalog_number(&self) -> Result<Option<String>, MmcSubchannelError> {
        let data = self.read_subchannel(AddressFormat::Lba, SubchannelParameter::Mcn)?;
        let input = &mut data.as_slice();
        parse_header(input)?;

        let format_code =
            u8::<_, ContextError>.verify(|code| *code == SubchannelParameter::Mcn.discriminant());
        let mcval = bits(bool::<_, ContextError>);
        let (_format_code, _, mcval, mcn, _zero, _aframe) = (
            format_code,
            take(RESERVED1_SIZE),
            mcval,
            take(MCN_SIZE),
            u8.verify(|zero| *zero == 0),
            u8, // aframe
        )
            .context(StrContext::Label("media catalog number descriptor"))
            .parse_next(input)?;

        if !mcval {
            return Ok(None);
        }
        let mcn =
            String::from_utf8(mcn.to_vec()).expect("mcn should not contain non utf8 characters");

        return Ok(Some(mcn));

        const RESERVED1_SIZE: usize = 3; // the field starts at index 1
        const MCN_SIZE: usize = 13;
    }

    /// Get the International Standard Recording Code (ISRC) of given track number.
    pub fn isrc(&self, track_number: TrackNumber) -> Result<Option<String>, MmcSubchannelError> {
        let param = SubchannelParameter::Isrc {
            track_number: track_number.0,
        };
        let data = self.read_subchannel(AddressFormat::Lba, param)?;
        let input = &mut data.as_slice();
        parse_header(input)?;

        let format_code = u8::<_, ContextError>.verify(|code| *code == param.discriminant());
        let adr_and_control = bits((
            bits_take::<_, u8, _, ContextError>(ADR_BITS),
            bits_take::<_, u8, _, _>(CONTROL_BITS),
        ));
        let tcval = bits(bool::<_, ContextError>);
        let (_format_code, _adr_and_control, _track, _, tcval, isrc, _zero, _aframe) = (
            format_code,
            adr_and_control,
            u8.verify(|track| *track == track_number.0),
            take(RESERVED3_SIZE),
            tcval,
            take(ISRC_SIZE),
            u8.verify(|zero| *zero == 0),
            u8, // aframe
        )
            .context(StrContext::Label("isrc descriptor"))
            .parse_next(input)?;

        if !tcval {
            return Ok(None);
        }
        let isrc =
            String::from_utf8(isrc.to_vec()).expect("isrc should not contain non utf8 characters");

        return Ok(Some(isrc));

        const ADR_BITS: usize = 4;
        const CONTROL_BITS: usize = 4;
        const RESERVED3_SIZE: usize = 1;
        const ISRC_SIZE: usize = 12;
    }

    /// Get the current position of the disc in time units.
    pub fn cd_current_position(&self) -> Result<CdCurrentPosition, MmcSubchannelError> {
        let param = SubchannelParameter::CdCurrentPosition;
        let data = self.read_subchannel(AddressFormat::Time, param)?;
        let input = &mut data.as_slice();
        parse_header(input)?;

        let format_code = u8::<_, ContextError>.verify(|code| *code == param.discriminant());
        let adr_and_control = bits((
            bits_take::<_, u8, _, ContextError>(ADR_BITS),
            bits_take::<_, u8, _, _>(CONTROL_BITS),
        ));
        let (_, _adr_and_control, track, index, absolute_address, relative_address) = (
            format_code,
            adr_and_control,
            u8, // track
            u8, // index
            take(ABSOLUTE_ADDR_SIZE),
            take(RELATIVE_ADDR_SIZE),
        )
            .context(StrContext::Label("cd current position descriptor"))
            .parse_next(input)?;

        let absolute_position = TimePosition {
            hour: absolute_address[0],
            minute: absolute_address[1],
            second: absolute_address[2],
            frame: absolute_address[3],
        };
        let relative_position = TimePosition {
            hour: relative_address[0],
            minute: relative_address[1],
            second: relative_address[2],
            frame: relative_address[3],
        };

        return Ok(CdCurrentPosition {
            track,
            index,
            absolute_position,
            relative_position,
        });

        const ADR_BITS: usize = 4;
        const CONTROL_BITS: usize = 4;
        const ABSOLUTE_ADDR_SIZE: usize = 4;
        const RELATIVE_ADDR_SIZE: usize = 4;
    }

    /// Perform an MMC `READ SUB-CHANNEL`.
    fn read_subchannel(
        &self,
        address_format: AddressFormat,
        param: SubchannelParameter,
    ) -> Result<SubchannelData, MmcError> {
        let mut data = SubchannelData::default();
        let mut cdb = Cdb::default();
        cdb[0] = OPCODE;
        if let AddressFormat::Time = address_format {
            cdb[1] |= 1 << TIME_BITPOS;
        }
        cdb[2] |= 1 << SUBQ_BITPOS; // always include Q sub-channel data
        cdb[3] = param.discriminant(); // parameter list
        if let SubchannelParameter::Isrc { track_number } = param {
            cdb[6] = track_number;
        }
        cdb[7..9].copy_from_slice(&(data.len() as u16).to_be_bytes());

        self.run_command(Some(MmcDirection::Read), &mut data, cdb)?;

        return Ok(data);

        const OPCODE: u8 = 0x42;
        const SUBQ_BITPOS: usize = 6;
        const TIME_BITPOS: usize = 1;
    }
}

/// The status of audio play operations
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, TryFromPrimitive)]
pub enum MmcAudioStatus {
    PlayInProgress = 0x11,
    PlayPaused = 0x12,
    PlayCompleted = 0x13,
    PlayStopped = 0x14,

    #[default]
    NoStatus = 0x15,
}

/// error getting audio status via `READ SUB-CHANNEL`
#[derive(Debug, Display, Error)]
pub enum MmcAudioStatusError {
    /// The device not support reporting audio status
    NotSupported,

    /// operating system returned an error: {0}
    Cmd(#[from] MmcError),

    /// invalid response from command
    InvalidResponse(String),
}
impl From<MmcSubchannelError> for MmcAudioStatusError {
    fn from(value: MmcSubchannelError) -> Self {
        match value {
            MmcSubchannelError::Cmd(error) => Self::Cmd(error),
            MmcSubchannelError::InvalidResponse(error) => Self::InvalidResponse(error),
        }
    }
}

/// Format to use for address fields in the response of `READ SUB-CHANNEL`
#[allow(unused)]
enum AddressFormat {
    Lba,
    Time,
}

#[repr(u8)]
#[allow(unused)]
#[derive(Clone, Copy, Debug)]
enum SubchannelParameter {
    CdCurrentPosition = 0x1,
    Mcn = 0x2,
    Isrc { track_number: u8 } = 0x3,
}
impl SubchannelParameter {
    fn discriminant(&self) -> u8 {
        // SAFETY: Because `Self` is marked `repr(u8)`, its layout is a `repr(C)` `union`
        // between `repr(C)` structs, each of which has the `u8` discriminant as its first
        // field, so we can read the discriminant without offsetting the pointer.
        // Source:
        // https://doc.rust-lang.org/stable/std/mem/fn.discriminant.html#accessing-the-numeric-value-of-the-discriminant
        unsafe { *(&raw const *self).cast::<u8>() }
    }
}

type SubchannelData = [u8; 24];

fn parse_header(input: &mut &[u8]) -> Result<Option<MmcAudioStatus>, MmcSubchannelError> {
    debug!(header = ?input, "parse_header()");
    let (_, audio_status, remainder) = (u8::<_, ContextError>, u8, length_take(be_u16))
        .context(StrContext::Label("READ SUB-CHANNEL response header"))
        .parse_next(input)?;
    *input = remainder;

    (audio_status != 0)
        .then(|| MmcAudioStatus::try_from(audio_status))
        .transpose()
        .map_err(MmcSubchannelError::from)
}

/// error from a `READ SUB-CHANNEL` command
#[non_exhaustive]
#[derive(Debug, Display, Error)]
pub enum MmcSubchannelError {
    /// operating system returned an error
    Cmd(#[from] MmcError),

    /// invalid response from mmc command: {0}
    InvalidResponse(String),
}
impl From<ContextError> for MmcSubchannelError {
    fn from(err: ContextError) -> Self {
        Self::InvalidResponse(err.to_string())
    }
}
impl<T: TryFromPrimitive> From<TryFromPrimitiveError<T>> for MmcSubchannelError {
    fn from(err: TryFromPrimitiveError<T>) -> Self {
        Self::InvalidResponse(err.to_string())
    }
}

/// Track number.
///
/// Values must be between 1 and 99.
/// Use [`TrackNumber::try_from`] to construct a new value.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TrackNumber(u8);
impl TryFrom<u8> for TrackNumber {
    type Error = InvalidTrackNumber;

    /// Construct `Self` from given track number.
    ///
    /// # Errors
    /// If the track number not within the range of 1 to 99, inclusive.
    fn try_from(track_number: u8) -> Result<Self, Self::Error> {
        if (1..=99).contains(&track_number) {
            Ok(Self(track_number))
        } else {
            Err(InvalidTrackNumber(track_number))
        }
    }
}
impl Default for TrackNumber {
    fn default() -> Self {
        Self(1)
    }
}
/// invalid track number '{0}'; value must be between 1 and 99
#[derive(Debug, Display, Error)]
pub struct InvalidTrackNumber(u8);

/// CD current position information.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CdCurrentPosition {
    /// track number
    pub track: u8,

    /// index number
    pub index: u8,

    /// position relative to the logical beginning of the media
    pub absolute_position: TimePosition,

    /// position relative to the logical beginning of the current track
    pub relative_position: TimePosition,
}
/// A CD position, expressed in time units
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TimePosition {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub frame: u8,
}

#[cfg(test)]
mod tests {
    use tracing::info;

    use super::*;

    #[test_log::test(test)]
    #[ignore = "requires a disc drive with mmc"]
    fn audio_status() {
        let audio_status = Mmc::new().unwrap().audio_status();
        info!(?audio_status);
        assert!(matches!(
            audio_status,
            Ok(_) | Err(MmcAudioStatusError::NotSupported)
        ));
    }

    #[test_log::test(test)]
    #[ignore = "requires a disc drive with mmc"]
    fn media_catalog_number() {
        let mcn = Mmc::new().unwrap().media_catalog_number().unwrap();
        info!(?mcn);
    }

    #[test_log::test(test)]
    #[ignore = "requires a disc drive with mmc"]
    fn isrc() {
        let isrc = Mmc::new().unwrap().isrc(TrackNumber::default()).unwrap();
        info!(?isrc);
    }

    #[test_log::test(test)]
    #[ignore = "requires a disc drive with mmc"]
    fn cd_current_position() {
        let cd_current_position = Mmc::new().unwrap().cd_current_position().unwrap();
        info!(?cd_current_position);
    }
}
