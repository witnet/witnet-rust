use chrono::{prelude::*, TimeZone};
use lazy_static::lazy_static;
use ntp;
use serde::{Deserialize, Serialize};
use std::sync::RwLock;
use std::time::Duration;

/// NTP Timestamp difference
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
pub struct NTPDiff {
    /// Difference between NTP and system timestamp
    pub ntp_diff: Duration,
    /// Flag to indicate if NTP is bigger or smaller
    /// than system timestamp
    pub bigger: bool,
}

lazy_static! {
    static ref NTP_TS: RwLock<NTPDiff> = RwLock::new(NTPDiff::default());
}

/// Get NTP timestamp
fn get_ntp_diff() -> NTPDiff {
    NTP_TS.read().expect("Timestamp with poisoned lock").clone()
}

/// Set NTP timestamp
fn get_mut_ntp_diff() -> std::sync::RwLockWriteGuard<'static, NTPDiff> {
    NTP_TS.write().expect("Timestamp with poisoned lock")
}

/// Get Local timestamp
pub fn get_local_timestamp() -> (i64, u32) {
    // Get UTC current datetime
    let utc: DateTime<Utc> = Utc::now();

    let utc_secs = utc.timestamp();
    let utc_subsec_nanos = utc.timestamp_subsec_nanos();

    (utc_secs, utc_subsec_nanos)
}

/// Update NTP timestamp
pub fn update_global_timestamp(addr: &str) {
    match get_timestamp_ntp(addr) {
        Ok(ntp) => {
            let utc = get_local_timestamp();
            let mut ntp_diff = get_mut_ntp_diff();

            if let Some(diff) = duration_between_timestamps(utc, ntp) {
                ntp_diff.ntp_diff = diff;
                log::debug!("Update NTP -> Our UTC is {} seconds before", diff.as_secs());
                ntp_diff.bigger = true;
            } else {
                let diff = duration_between_timestamps(ntp, utc).unwrap();
                ntp_diff.ntp_diff = diff;
                log::debug!("Update NTP -> Our UTC is {} seconds after", diff.as_secs());
                ntp_diff.bigger = false;
            }
        }
        Err(e) => {
            log::warn!("NTP request failed: {}", e);
        }
    }
}

fn local_time(timestamp: ntp::protocol::TimestampFormat) -> chrono::DateTime<chrono::Local> {
    let unix_time = ntp::unix_time::Instant::from(timestamp);
    chrono::Local.timestamp(unix_time.secs(), unix_time.subsec_nanos() as u32)
}
/// Get NTP timestamp from an addr specified
pub fn get_timestamp_ntp(addr: &str) -> Result<(i64, u32), std::io::Error> {
    ntp::request(addr).map(|p| {
        let ts = local_time(p.receive_timestamp);

        (ts.timestamp(), ts.timestamp_subsec_nanos())
    })
}
/// Function to get timestamp from system/ntp server as UTC Unix timestamp, seconds since Unix epoch
pub fn get_timestamp() -> i64 {
    get_timestamp_nanos().0
}

/// Function to get timestamp from system/ntp server as UTC Unix timestamp, seconds and nanoseconds since Unix epoch
pub fn get_timestamp_nanos() -> (i64, u32) {
    let utc_ts = get_local_timestamp();
    let ntp_diff = get_ntp_diff();

    // Apply difference respect to NTP timestamp
    let utc_dur = Duration::new(utc_ts.0 as u64, utc_ts.1);
    let result = if ntp_diff.bigger {
        utc_dur.checked_add(ntp_diff.ntp_diff)
    } else {
        utc_dur.checked_sub(ntp_diff.ntp_diff)
    };

    match result {
        Some(x) => (x.as_secs() as i64, x.subsec_nanos()),
        None => panic!(
            "Error: Overflow in timestamp\n\
             UTC timestamp: {} secs, {} nanosecs\n\
             NTP diff: {:?}",
            utc_ts.0, utc_ts.1, ntp_diff
        ),
    }
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

/// Convert seconds to a human readable format like "2h 46m 40s"
pub fn seconds_to_human_string(x: u64) -> String {
    let seconds_in_one_day = 60 * 60 * 24;
    // 1 year = 365.25 days
    let seconds_in_one_year = seconds_in_one_day * 365 + seconds_in_one_day / 4;

    // If x > 1 year, do not show hours
    let x = if x >= seconds_in_one_year {
        // 1 year = 365.25 days, we must add 0.25 days to force hours to zero
        x - (x % seconds_in_one_day) + (seconds_in_one_day / 4)
    } else {
        x
    };

    humantime::format_duration(Duration::from_secs(x)).to_string()
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

    #[test]
    fn human_duration() {
        let seconds_in_one_day = 60 * 60 * 24;
        // 1 year = 365.25 days
        let seconds_in_one_year = seconds_in_one_day * 365 + seconds_in_one_day / 4;

        assert_eq!(seconds_to_human_string(0), "0s");
        assert_eq!(seconds_to_human_string(10_000), "2h 46m 40s");
        assert_eq!(seconds_to_human_string(1_000_000), "11days 13h 46m 40s");
        assert_eq!(seconds_to_human_string(seconds_in_one_year), "1year");
        assert_eq!(seconds_to_human_string(seconds_in_one_year + 1), "1year");
        // This may look incorrect, but according to humantime, 1 month == 30.44 days, so...
        assert_eq!(
            seconds_to_human_string(seconds_in_one_year - 1),
            "11months 30days 9h 50m 23s"
        );
        // If you convert everything to days it makes sense
        assert_eq!(
            seconds_to_human_string(seconds_in_one_day * 366 - 1),
            "1year"
        );
        assert_eq!(
            seconds_to_human_string(seconds_in_one_day * 366),
            "1year 1day"
        );
        assert_eq!(
            seconds_to_human_string(seconds_in_one_day * 366 + 1),
            "1year 1day"
        );
    }
}
