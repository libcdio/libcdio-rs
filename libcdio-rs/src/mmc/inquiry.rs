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

//! Routines based on MMC `INQUIRY`.
//! This command is described in SPC-3.

use displaydoc::Display;
use thiserror::Error;
use tracing::debug;

use crate::{
    Mmc,
    mmc::{Cdb, MmcCommand, MmcDirection, MmcError},
};

/// Routines based on MMC `INQUIRY`.
impl Mmc {
    /// Get the hardware identifiers (Product, Vendor and Revision).
    pub fn hardware_identifiers(&self) -> Result<HardwareIdentifiers, GetHardwareIdentifiersError> {
        let buf = self.inquiry(InquiryKind::Standard)?;
        let response_data_format = buf[RESPONSE_DATA_FMT_POS] & 0b1111;
        if response_data_format != RESPONSE_DATA_FMT_MMC {
            return Err(MmcInquiryError::InvalidResponse(format!(
                "unsupported non-standard INQUIRY response format, got response data format of {}",
                response_data_format,
            ))
            .into());
        }
        let vendor = String::from_utf8_lossy(&buf[VENDOR_ID_START_POS..=VENDOR_ID_END_POS])
            .trim()
            .to_string();
        let product = String::from_utf8_lossy(&buf[PRODUCT_ID_START_POS..=PRODUCT_ID_END_POS])
            .trim()
            .to_string();
        let revision = String::from_utf8_lossy(&buf[REVISION_START_POS..=REVISION_END_POS])
            .trim()
            .to_string();

        return Ok(HardwareIdentifiers {
            product,
            vendor,
            revision,
        });

        const RESPONSE_DATA_FMT_POS: usize = 3;
        const RESPONSE_DATA_FMT_MMC: u8 = 2;
        const VENDOR_ID_START_POS: usize = 8;
        const VENDOR_ID_END_POS: usize = 15;
        const PRODUCT_ID_START_POS: usize = 16;
        const PRODUCT_ID_END_POS: usize = 31;
        const REVISION_START_POS: usize = 32;
        const REVISION_END_POS: usize = 35;
    }

    fn inquiry(&self, kind: InquiryKind) -> Result<Vec<u8>, MmcInquiryError> {
        let mut buf = vec![0; ALLOC_LEN];
        let mut cdb = Cdb::default();
        cdb[0] = MmcCommand::Inquiry as u8;
        if let InquiryKind::VitalProductData { page_code } = kind {
            cdb[1] = EVPD_SET;
            cdb[2] = page_code;
        }
        cdb[3..=4].copy_from_slice(&(buf.len() as u16).to_be_bytes());

        self.run_command(Some(MmcDirection::Read), buf.as_mut_slice(), cdb)?;
        debug!(?buf, len = buf.len());

        return Ok(buf);

        const ALLOC_LEN: usize = 255;
        const EVPD_SET: u8 = 1;
    }

    /// Does the device support MMC.
    ///
    /// Per SCSI, a device that responds to an SPC `INQUIRY` with a
    /// `Peripheral device type` of `CD/DVD device` is expected
    /// to implement MMC.
    pub(crate) fn is_mmc_device(&self) -> Result<bool, MmcInquiryError> {
        let response = self.inquiry(InquiryKind::Standard)?;
        let peripheral_device_type = response[0] & PERIPHERAL_DEVICE_TYPE_BITMASK;
        return Ok(peripheral_device_type == PERIPHERAL_DEVICE_TYPE_CD_DVD);

        const PERIPHERAL_DEVICE_TYPE_BITMASK: u8 = 0b11111;
        const PERIPHERAL_DEVICE_TYPE_CD_DVD: u8 = 0x05;
    }
}

/// could not get hardware identifiers
#[derive(Debug, Display, Error)]
pub struct GetHardwareIdentifiersError {
    #[from]
    pub source: MmcInquiryError,
}

/// error from an `INQUIRY` command.
#[derive(Debug, Display, Error)]
pub enum MmcInquiryError {
    /// operating system returned an error
    Os(#[from] MmcError),

    /// invalid response from mmc command: {0}
    InvalidResponse(String),
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
/// Basic hardware identifiers
pub struct HardwareIdentifiers {
    pub product: String,
    pub vendor: String,
    pub revision: String,
}

#[allow(unused)]
#[derive(Clone, Copy, Debug)]
enum InquiryKind {
    Standard,
    VitalProductData { page_code: u8 },
}

#[cfg(test)]
mod tests {
    use tracing::info;

    use super::*;

    #[test_log::test(test)]
    #[ignore = "requires a drive with mmc"]
    fn hardware_identifiers() {
        let hardware_identifiers = Mmc::new().unwrap().hardware_identifiers().unwrap();
        info!(?hardware_identifiers);
    }
}
