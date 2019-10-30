use chrono::prelude::*;
use std::time::Duration;

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

/// Duration needed to wait from now until the target timestamp
pub fn duration_until_timestamp(target_secs: i64, target_nanos: u32) -> Option<Duration> {
    let (timestamp_now, timestamp_nanos) = get_timestamp_nanos();

    duration_between_timestamps(
        (timestamp_now, timestamp_nanos),
        (target_secs, target_nanos),
    )
}

/// Duration needed to wait from the first timestamp until the second timestamp
pub fn duration_between_timestamps(
    (now_secs, now_nanos): (i64, u32),
    (target_secs, target_nanos): (i64, u32),
) -> Option<Duration> {
    let (target_secs, now_secs) = (target_secs as u64, now_secs as u64);

    Duration::new(target_secs, target_nanos).checked_sub(Duration::new(now_secs, now_nanos))
}

/// Function for pretty printing a timestamp as a human friendly date and time
pub fn pretty_print(seconds: i64, nanoseconds: u32) -> String {
    Utc.timestamp(seconds, nanoseconds).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pretty_print_test() {
        let result = pretty_print(0, 0);
        let expected = "1970-01-01 00:00:00 UTC";

        assert_eq!(result, expected);
    }

    #[test]
    fn duration_between() {
        let a = (1, 0);
        let b = (2, 0);

        assert_eq!(duration_between_timestamps(a, a), Some(Duration::new(0, 0)));
        assert_eq!(duration_between_timestamps(a, b), Some(Duration::new(1, 0)));
        assert_eq!(duration_between_timestamps(b, a), None);

        let c = (0, 1);
        let d = (0, 2);

        assert_eq!(duration_between_timestamps(c, c), Some(Duration::new(0, 0)));
        assert_eq!(duration_between_timestamps(c, d), Some(Duration::new(0, 1)));
        assert_eq!(duration_between_timestamps(d, c), None);

        let e = (0, 999_999_999);
        let f = (1, 0);

        assert_eq!(duration_between_timestamps(e, e), Some(Duration::new(0, 0)));
        assert_eq!(duration_between_timestamps(e, f), Some(Duration::new(0, 1)));
        assert_eq!(duration_between_timestamps(f, e), None);
    }
}
