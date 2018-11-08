use chrono::prelude::*;

/// Function to get timestamp from system as UTC Unix timestamp, seconds since Unix epoch
pub fn get_timestamp() -> i64 {
    // Get UTC current datetime
    let utc: DateTime<Utc> = Utc::now();

    // Return number of non-leap seconds since Unix epoch
    utc.timestamp()
}
