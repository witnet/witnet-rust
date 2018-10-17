use std::time::{SystemTime, SystemTimeError, UNIX_EPOCH};

use crate::error::{WitnetError, WitnetResult};

/// Function to get timestamp from system as UTC
pub fn get_timestamp() -> WitnetResult<u64, SystemTimeError> {
    // Get timestamp from system
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(value) => Ok(value.as_secs()),
        Err(e) => Err(WitnetError::from(e)),
    }
}