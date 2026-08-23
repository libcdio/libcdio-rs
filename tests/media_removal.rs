//! Tests that would remove the drive media
use libcdio_rs::{
    Mmc,
    mmc::{MmcCloseTrayError, MmcError, MmcSenseData, MmcStartStopError, SenseKey},
};

#[test]
#[ignore = "requires a drive with mmc"]
fn media_removal() {
    if std::env::var("MANUAL_TESTS").is_err() {
        return;
    }
    let mmc = Mmc::new().unwrap();

    mmc.prevent_media_removal().unwrap();
    mmc.eject().unwrap_err(); // eject should fail

    mmc.allow_media_removal().unwrap();
    mmc.eject().unwrap(); // eject should pass

    // if present, the tray should close
    let res = mmc.close_tray();
    assert!(matches!(
        res,
        Ok(())
            | Err(MmcCloseTrayError {
                source: MmcStartStopError {
                    // Sense data corresponding to MMC error "INVALID FIELD IN CDB"
                    // is returned if the device does not have a tray
                    source: MmcError::CheckCondition(MmcSenseData {
                        sense_key: SenseKey::IllegalRequest,
                        asc: 0x24,
                        ascq: 0x00,
                        ..
                    },),
                }
            })
    ));
}
