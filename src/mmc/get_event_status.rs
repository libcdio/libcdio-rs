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

//! Routines based on MMC `GET EVENT STATUS NOTIFICATION`.

use std::time::Duration;

use bitflags::bitflags;
use displaydoc::Display;
use num_enum::{TryFromPrimitive, TryFromPrimitiveError};
use thiserror::Error;
use tracing::debug;
use winnow::{
    Parser,
    binary::{
        be_u16,
        bits::{bits, bool, take as bits_take},
        length_and_then, u8,
    },
    combinator::{preceded, separated_pair},
    error::{ContextError, StrContext},
    token::{rest, take},
};

use crate::{
    Mmc,
    mmc::{Cdb, MmcDirection, MmcError},
};

/// Routines based on MMC `GET EVENT STATUS NOTIFICATION`.
impl Mmc {
    /// Get all the event classes supported by the device.
    pub fn supported_events(&self) -> Result<EventClass, MmcStatusError> {
        let data = self.get_event_status_notification(EventMode::Polled, EventClass::empty())?;
        let supported_events = parse_header(EventClass::empty(), &mut data.as_slice())?;

        Ok(supported_events)
    }

    /// Perform an MMC `GET EVENT STATUS NOTIFICATION`.
    fn get_event_status_notification(
        &self,
        mode: EventMode,
        class: EventClass,
    ) -> Result<EventData, MmcError> {
        let mut data = EventData::default();
        let mut cdb = Cdb::default();
        cdb[0] = OPCODE;
        cdb[1] = if mode == EventMode::Polled { 1 } else { 0 };
        cdb[4] = class.bits();
        cdb[7..9].copy_from_slice(&(data.len() as u16).to_be_bytes());

        self.run_command(Some(MmcDirection::Read), &mut data, cdb)?;

        return Ok(data);

        const OPCODE: u8 = 0x4A;
    }

    /// Is the device tray open.
    ///
    /// If the device does not have a tray, this should still return `false`.
    pub fn is_tray_open(&self) -> Result<bool, MmcStatusError> {
        self.media_status()
            .map(|status| status.state.contains(MediaState::DoorOrTrayOpen))
    }

    /// Get operational change status from the device.
    pub fn operational_event_status(&self) -> Result<OperationalStatus, MmcStatusError> {
        let data =
            self.get_event_status_notification(EventMode::Polled, EventClass::OperationalChange)?;
        let input = &mut data.as_slice();
        parse_header(EventClass::OperationalChange, input)?;

        // skip the legacy 'operational status' field
        let prevent_bit_and_op_status = bits(separated_pair(
            bool,
            take(3_usize),
            bits_take::<_, u8, _, ContextError>(4_usize),
        ));
        // skip the 'event code' field as the 'operational change' field
        // provides the same
        let (_, (prevent_bit, _op_status), op_change) =
            (parse_event_code, prevent_bit_and_op_status, be_u16)
                .context(StrContext::Label("operational change event descriptor"))
                .parse_next(input)?;
        let change = (op_change != 0)
            .then(|| OperationalEvent::try_from(op_change))
            .transpose()?;

        Ok(OperationalStatus {
            event: change,
            persistent_prevent: prevent_bit,
        })
    }

    /// Get power management status from the device.
    pub fn power_status(&self) -> Result<PowerStatus, MmcStatusError> {
        let data =
            self.get_event_status_notification(EventMode::Polled, EventClass::PowerManagement)?;
        let input = &mut data.as_slice();
        parse_header(EventClass::PowerManagement, input)?;

        let (event_code, status) = (parse_event_code, u8)
            .context(StrContext::Label("power management event descriptor"))
            .parse_next(input)?;
        let event = (event_code != 0)
            .then(|| PowerEvent::try_from(event_code))
            .transpose()?;
        let state = PowerState::try_from(status)?;

        Ok(PowerStatus { event, state })
    }

    /// Get external request status from the device.
    pub fn external_status(&self) -> Result<ExternalRequestStatus, MmcStatusError> {
        let data =
            self.get_event_status_notification(EventMode::Polled, EventClass::ExternalRequest)?;
        let input = &mut data.as_slice();
        parse_header(EventClass::ExternalRequest, input)?;

        // skip the 'persistent prevented' bit as the 'external request status'
        // field provides the same
        let status = bits(preceded(
            bits_take::<_, u8, _, ContextError>(4_usize),
            bits_take::<_, u8, _, _>(4_usize),
        ));
        let (event_code, status, request) = (parse_event_code, status, be_u16)
            .context(StrContext::Label("external request event descriptor"))
            .parse_next(input)?;
        let event = (event_code != 0)
            .then(|| ExternalRequestEvent::try_from(event_code))
            .transpose()?;
        let state = ExternalRequestState::try_from(status)?;
        let request = (request != 0)
            .then(|| ExternalRequest::try_from(request))
            .transpose()?;

        Ok(ExternalRequestStatus {
            event,
            state,
            request,
        })
    }

    /// Get media status from the device.
    pub fn media_status(&self) -> Result<MediaStatus, MmcStatusError> {
        let data = self.get_event_status_notification(EventMode::Polled, EventClass::Media)?;
        let input = &mut data.as_slice();
        parse_header(EventClass::Media, input)?;

        let (event_code, status, start_slot, end_slot) = (parse_event_code, u8, u8, u8)
            .context(StrContext::Label("media event descriptor"))
            .parse_next(input)?;
        let event = (event_code != 0)
            .then(|| MediaEvent::try_from(event_code))
            .transpose()?;
        let state = MediaState::from_bits_truncate(status);

        Ok(MediaStatus {
            event,
            state,
            start_slot,
            end_slot,
        })
    }

    /// Get multiple host event status from the device.
    pub fn multihost_status(&self) -> Result<MultiHostStatus, MmcStatusError> {
        let data = self.get_event_status_notification(EventMode::Polled, EventClass::MultiHost)?;
        let input = &mut data.as_slice();
        parse_header(EventClass::MultiHost, input)?;

        // skip the 'persistent prevented' bit as the 'multiple host status'
        // field provides the same
        let status = bits(preceded(
            bits_take::<_, u8, _, ContextError>(4_usize),
            bits_take::<_, u8, _, _>(4_usize),
        ));
        let (event_code, status, priority) = (parse_event_code, status, be_u16)
            .context(StrContext::Label("multiple host event descriptor"))
            .parse_next(input)?;
        let event = (event_code != 0)
            .then(|| MultiHostEvent::try_from(event_code))
            .transpose()?;
        let state = MultiHostState::try_from(status)?;
        let priority = (priority != 0)
            .then(|| MultiHostPriority::try_from(priority))
            .transpose()?;

        Ok(MultiHostStatus {
            event,
            state,
            priority,
        })
    }

    /// Get busy status from the device.
    pub fn busy_status(&self) -> Result<BusyStatus, MmcStatusError> {
        let data = self.get_event_status_notification(EventMode::Polled, EventClass::DeviceBusy)?;
        let input = &mut data.as_slice();
        parse_header(EventClass::DeviceBusy, input)?;

        let (event_code, status, time) = (parse_event_code, u8, be_u16)
            .context(StrContext::Label("device busy event descriptor"))
            .parse_next(input)?;
        let event = (event_code != 0)
            .then(|| BusyEvent::try_from(event_code))
            .transpose()?;
        let state = BusyState::try_from(status)?;
        let time = (state != BusyState::NotBusy).then(|| Duration::from_millis(u64::from(time)));

        Ok(BusyStatus {
            event,
            state,
            busy_time: time,
        })
    }
}
type EventData = [u8; 12];

bitflags! {
    /// Notification class of `GET EVENT STATUS NOTIFICATION` command
    #[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
    pub struct EventClass: u8 {
        /// Change of operational capabilities or parameters for the drive.
        const OperationalChange = 1 << 1;
        /// Changes to power status.
        const PowerManagement = 1 << 2;
        /// External requests such as a remote or a button.
        const ExternalRequest = 1 << 3;
        /// Media related changes
        const Media = 1 << 4;
        /// Requests for control by other hosts.
        const MultiHost = 1 << 5;
        /// Commands that are executing but require a long time to complete.
        const DeviceBusy = 1 << 6;
    }
}

/// error from a `GET EVENT STATUS NOTIFICATION` command
#[non_exhaustive]
#[derive(Debug, Display, Error)]
pub enum MmcStatusError {
    /// error performing MMC command
    Cmd(#[from] MmcError),

    /// invalid response from mmc command: {0}
    InvalidResponse(String),

    /// device does not support: {0:?}
    EventNotSupported(EventClass),
}

impl From<ContextError> for MmcStatusError {
    fn from(err: ContextError) -> Self {
        Self::InvalidResponse(err.to_string())
    }
}

impl<T: TryFromPrimitive> From<TryFromPrimitiveError<T>> for MmcStatusError {
    fn from(err: TryFromPrimitiveError<T>) -> Self {
        Self::InvalidResponse(err.to_string())
    }
}

/// Operation mode of `GET EVENT STATUS NOTIFICATION` command
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum EventMode {
    /// Asynchronous events
    #[allow(unused)]
    Async,

    /// Polled events
    #[default]
    Polled,
}

/// Validate the header with the expected class and return the supported events
fn parse_header(
    expected_class: EventClass,
    input: &mut &[u8],
) -> Result<EventClass, MmcStatusError> {
    debug!(header = ?input, "parse_header()");
    let nea_and_notif_class = bits(separated_pair::<_, _, u8, u8, ContextError, _, _, _>(
        bool,
        bits_take(4_usize),
        bits_take(3_usize),
    ));
    let ((nea, notif_class), supported_events, remaining) =
        length_and_then(be_u16, (nea_and_notif_class, u8, rest))
            .context(StrContext::Label(
                "GET EVENT STATUS NOTIFICATION response header",
            ))
            .parse_next(input)?;
    *input = remaining;
    if !expected_class.is_empty() && nea {
        return Err(MmcStatusError::EventNotSupported(expected_class));
    }
    let event_class = 1 << notif_class;
    if !expected_class.is_empty() && event_class != expected_class.bits() {
        return Err(MmcStatusError::InvalidResponse(format!(
            "invalid event code, expected 0b{:b} got 0b{:b}",
            expected_class.bits(),
            event_class
        )));
    }

    Ok(EventClass::from_bits_truncate(supported_events))
}

/// Status of operational changes to the device
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OperationalStatus {
    /// Most recent operational change event on the device
    pub event: Option<OperationalEvent>,

    /// Persistent prevent state is active.
    ///
    /// Upon entering the Persistent Prevent state, the Drive shall disable any eject mechanisms, and all media after
    /// initial media spin up shall remain locked in the Drive until the Host issues an eject request, or the Persistent
    /// Prevent status is reset and the hardware eject mechanism again becomes available.
    pub persistent_prevent: bool,
}
/// Source of operational changes to the device
#[repr(u16)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, TryFromPrimitive)]
pub enum OperationalEvent {
    /// An unspecified event may have changed feature currency
    #[default]
    FeatureChange = 0x1,

    /// The feature list may have added current features
    NewFeatures = 0x2,

    /// The logical unit has been reset
    Reset = 0x3,

    /// The logical unit's microcode may have changed
    FirmwareChanged = 0x4,

    /// The logical unit's identification information may have changed
    InquiryChange = 0x5,
}

/// Take a byte and interpret the lowest four bits
fn parse_event_code(input: &mut &[u8]) -> winnow::Result<u8> {
    bits(preceded(
        take::<_, _, ContextError>(4_usize),
        bits_take(4_usize),
    ))
    .context(StrContext::Label("event code"))
    .parse_next(input)
}

/// Changes to power status
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PowerStatus {
    /// Most recent power management event on the device
    pub event: Option<PowerEvent>,

    /// Current power state of the drive
    pub state: PowerState,
}
/// The power change event
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, TryFromPrimitive)]
pub enum PowerEvent {
    /// The drive successfully changed to the specified power state
    #[default]
    PwrChgSuccessful = 0x1,

    /// The drive failed to enter the last requested state and is still
    /// operating at the power state specified in the `status` field
    PwrChgFail = 0x2,
}
/// The current power state of the drive
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, TryFromPrimitive)]
pub enum PowerState {
    /// Active
    #[default]
    Active = 0x1,

    /// Idle
    Idle = 0x2,

    /// Standby
    Standby = 0x3,

    /// The drive is about to enter Sleep
    Sleep = 0x4,
}

/// External requests such as a remote or a button
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExternalRequestStatus {
    /// Most recent external request event on the device
    pub event: Option<ExternalRequestEvent>,

    /// Device's ability to respond to the host
    pub state: ExternalRequestState,

    /// Operation requested or performed via an external request to the device
    pub request: Option<ExternalRequest>,
}
/// External request events to the device
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, TryFromPrimitive)]
pub enum ExternalRequestEvent {
    /// A front, back, or remote button has been depressed
    DriveKeyDown = 0x1,

    /// A front, back, or remote button has been released
    DriveKeyUp = 0x2,

    /// The drive has received a command from another host that requires an
    /// action that may interfere with the persistent prevent owner's
    /// operation
    #[default]
    ExternalRequestNotification = 0x3,
}
/// The device's ability to respond to the host
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, TryFromPrimitive)]
pub enum ExternalRequestState {
    /// The drive is ready for operation
    #[default]
    Ready = 0x0,

    /// Another host has an active persistent prevent
    OtherPrevent = 0x1,
}
/// Operation requested or performed via an external request to the device
#[repr(u16)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, TryFromPrimitive)]
pub enum ExternalRequest {
    /// The request queue has overflowed. External Request events may be lost.
    Overrun = 0x1,

    /// The play button was pressed or was requested by another host.
    Play = 0x101,

    /// The rewind/back button was pressed or was requested by another host.
    RewindOrBack = 0x102,

    /// The fast forward button was pressed or was requested by another host.
    FastForward = 0x103,

    /// The pause button was pressed or was requested by another host.
    Pause = 0x104,

    /// The stop button was pressed or was requested by another host.
    Stop = 0x106,

    /// A front panel button was pressed or was requested by another host.
    Ascii = 0x107,

    /// A vendor unique request
    #[default]
    #[num_enum(alternatives = [0xF001..=0xFFFF])]
    VendorUnique = 0xF000,
}

/// Media related changes
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MediaStatus {
    /// Most recent media event on the device
    pub event: Option<MediaEvent>,

    /// Current media state of the device
    pub state: MediaState,

    /// The first slot of a multiple slot drive the media status
    /// notification applies to.
    /// Only applies to drives that support multiple slots.
    pub start_slot: u8,

    /// The last slot of a multiple slot drive the media status
    /// notification applies to.
    /// Only applies to drives that support
    pub end_slot: u8,
}
/// Media event
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, TryFromPrimitive)]
pub enum MediaEvent {
    /// The drive has received a request from the user to eject the specified
    /// slot or media
    #[default]
    EjectRequest = 0x1,

    /// The specified slot has received new media and is ready to access it
    NewMedia = 0x2,

    /// The media has been removed from the specified slot and the drive is
    /// unable to access the media without user intervention.
    /// This applies to media changers only.
    MediaRemoval = 0x3,

    /// The user has requested that the media in the specified slot be loaded.
    /// This applies to media changers only.
    MediaChanged = 0x4,

    /// A DVD+RW background format has completed.
    BackgroundFormatCompleted = 0x5,

    /// A DVD+RW background format has been automatically restarted by the
    /// drive.
    BackgroundFormatRestarted = 0x6,
}
bitflags! {
    /// Media state
    #[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
    pub struct MediaState: u8 {
        /// The tray or door mechanism is in the open state.
        const DoorOrTrayOpen = 1;

        /// Media is present in the drive.
        const MediaPresent = 1 << 1;
    }
}

/// Requests for control by other hosts
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MultiHostStatus {
    /// Most recent multiple host event on the device
    pub event: Option<MultiHostEvent>,

    /// The drive's ability to respond to the host
    pub state: MultiHostState,

    /// Priority of tasks relative to the other host
    pub priority: Option<MultiHostPriority>,
}
/// Requests for drive control and state changes from other hosts
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, TryFromPrimitive)]
pub enum MultiHostEvent {
    /// Another host has requested drive control
    #[default]
    ControlRequest = 0x1,

    /// Another host has received drive control
    ControlGrant = 0x2,

    /// Another host has released drive control
    ControlRelease = 0x3,
}
/// Ability of the drive to respond to the host
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, TryFromPrimitive)]
pub enum MultiHostState {
    /// The drive is ready for operation
    #[default]
    Ready = 0x0,

    /// Another host has an active persistent prevent.
    OtherPrevent = 0x1,
}
/// Priority of tasks relative to the other host
#[repr(u16)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, TryFromPrimitive)]
pub enum MultiHostPriority {
    /// No tasks pending on the host
    #[default]
    Low = 0x1,

    /// No critical tasks pending on the host
    Medium = 0x2,

    /// There are critical tasks pending on the host
    High = 0x3,
}

/// Used to notify the host of commands that are executing but require an
/// abnormally long time to complete.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BusyStatus {
    /// Most recent device busy event on the device
    pub event: Option<BusyEvent>,

    /// Current busy state of the device
    pub state: BusyState,

    /// Predicted amount of time remaining for the device to become not busy
    /// if the drive is currently busy.
    pub busy_time: Option<Duration>,
}
/// Device busy events
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, TryFromPrimitive)]
pub enum BusyEvent {
    /// The drive busy state has changed
    #[default]
    Change = 0x1,

    /// The drive busy condition has been changed by a loading/unloading
    /// operation that is not caused by command execution
    LoChange = 0x2,
}
/// Busy state of the device
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, TryFromPrimitive)]
pub enum BusyState {
    /// The drive is not busy
    #[default]
    NotBusy = 0x0,

    /// The drive is busy
    Busy = 0x1,
}

#[cfg(test)]
mod tests {
    use super::*;

    use tracing::info;

    #[test_log::test(test)]
    #[ignore = "requires a disc drive with mmc"]
    fn supported_events() {
        Mmc::new().unwrap().supported_events().unwrap();
    }

    #[test_log::test(test)]
    #[ignore = "requires a disc drive with mmc"]
    fn is_tray_open() {
        let is_tray_open = Mmc::new().unwrap().is_tray_open();
        info!(?is_tray_open);
        assert!(matches!(
            is_tray_open,
            Ok(_) | Err(MmcStatusError::EventNotSupported(_))
        ));
    }

    #[test_log::test(test)]
    #[ignore = "requires a disc drive with mmc"]
    fn operational_status() {
        let status = Mmc::new().unwrap().operational_event_status();
        info!(?status);
        assert!(matches!(
            status,
            Ok(_) | Err(MmcStatusError::EventNotSupported(_))
        ));
    }

    #[test_log::test(test)]
    #[ignore = "requires a disc drive with mmc"]
    fn power_status() {
        let status = Mmc::new().unwrap().power_status();
        info!(?status);
        assert!(matches!(
            status,
            Ok(_) | Err(MmcStatusError::EventNotSupported(_))
        ));
    }

    #[test_log::test(test)]
    #[ignore = "requires a disc drive with mmc"]
    fn external_status() {
        let status = Mmc::new().unwrap().external_status();
        info!(?status);
        assert!(matches!(
            status,
            Ok(_) | Err(MmcStatusError::EventNotSupported(_))
        ));
    }

    #[test_log::test(test)]
    #[ignore = "requires a disc drive with mmc"]
    fn media_status() {
        let status = Mmc::new().unwrap().media_status();
        info!(?status);
        assert!(matches!(
            status,
            Ok(_) | Err(MmcStatusError::EventNotSupported(_))
        ));
    }

    #[test_log::test(test)]
    #[ignore = "requires a disc drive with mmc"]
    fn multihost_status() {
        let status = Mmc::new().unwrap().multihost_status();
        info!(?status);
        assert!(matches!(
            status,
            Ok(_) | Err(MmcStatusError::EventNotSupported(_))
        ));
    }

    #[test_log::test(test)]
    #[ignore = "requires a disc drive with mmc"]
    fn busy_status() {
        let status = Mmc::new().unwrap().busy_status();
        info!(?status);
        assert!(matches!(
            status,
            Ok(_) | Err(MmcStatusError::EventNotSupported(_))
        ));
    }
}
