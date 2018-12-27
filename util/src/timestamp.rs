use chrono::prelude::*;

/// Function to get timestamp from system as UTC Unix timestamp, seconds since Unix epoch
pub fn get_timestamp() -> i64 {
    // Get UTC current datetime
    let utc: DateTime<Utc> = Utc::now();

    // Return number of non-leap seconds since Unix epoch
    utc.timestamp()
}

/// Function to get timestamp from system as UTC Unix timestamp, seconds and nanoseconds since Unix epoch
pub fn get_timestamp_nanos() -> (i64, u32) {
    // Get UTC current datetime
    let utc: DateTime<Utc> = Utc::now();

    // Return number of non-leap seconds since Unix epoch and the number of nanoseconds since the last second boundary
    (utc.timestamp(), utc.timestamp_subsec_nanos())
}

/// Function for pretty printing a timestamp as a human friendly date and time
pub fn pretty_print(seconds: i64, nanoseconds: u32) -> String {
    Utc.timestamp(seconds, nanoseconds).to_string()
}

#[test]
fn pretty_print_test() {
    let result = pretty_print(0, 0);
    let expected = "1970-01-01 00:00:00 UTC";

    assert_eq!(result, expected);
}
