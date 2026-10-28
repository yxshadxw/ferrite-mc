//! Vanilla's `yyyy-MM-dd HH:mm:ss Z` timestamp format.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Duration, FixedOffset, Local};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Java's `SimpleDateFormat` pattern used by every vanilla admin file.
const TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H:%M:%S %z";

/// A vanilla timestamp, preserving the UTC offset it was written with.
///
/// Vanilla formats timestamps in the server's local timezone, so a file
/// written at `+0900` keeps `+0900` when it is loaded and saved again instead
/// of being converted to the current server's timezone.
///
/// ```
/// use ferrite_admin::Timestamp;
///
/// let timestamp = Timestamp::parse("2026-10-28 12:00:00 +0900").unwrap();
/// assert_eq!(timestamp.to_string(), "2026-10-28 12:00:00 +0900");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(DateTime<FixedOffset>);

impl Timestamp {
    /// Parses a timestamp such as `2026-10-28 12:00:00 +0900`.
    ///
    /// Offsets may also be written with a colon (`+09:00`); they are
    /// normalized to `+0900` when the timestamp is formatted.
    pub fn parse(value: &str) -> Result<Self, TimestampError> {
        DateTime::parse_from_str(value, TIMESTAMP_FORMAT)
            .map(Self)
            .map_err(|source| TimestampError {
                value: value.to_owned(),
                source,
            })
    }

    /// Returns the timestamp `duration` later, keeping the same offset.
    pub(crate) fn after(self, duration: Duration) -> Self {
        Self(self.0 + duration)
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0.format(TIMESTAMP_FORMAT))
    }
}

impl From<DateTime<FixedOffset>> for Timestamp {
    fn from(value: DateTime<FixedOffset>) -> Self {
        Self(value)
    }
}

impl FromStr for Timestamp {
    type Err = TimestampError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Timestamp::parse(&value).map_err(serde::de::Error::custom)
    }
}

/// Error returned when a timestamp string cannot be parsed.
#[derive(Debug, thiserror::Error)]
#[error("invalid timestamp {value:?}: {source}")]
pub struct TimestampError {
    value: String,
    #[source]
    source: chrono::ParseError,
}

/// Source of the current time.
///
/// [`AdminLists`](crate::AdminLists) uses this for new ban/op timestamps and
/// for expiry checks, so tests can move time around deterministically.
pub trait Clock: Send + Sync {
    /// Current time.
    fn now(&self) -> Timestamp;
}

/// The machine's local wall clock, matching vanilla's default timezone.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp(Local::now().fixed_offset())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positive_offset_round_trips() {
        let timestamp = Timestamp::parse("2026-10-28 12:00:00 +0900").unwrap();
        assert_eq!(timestamp.to_string(), "2026-10-28 12:00:00 +0900");
    }

    #[test]
    fn negative_and_utc_offsets_round_trip() {
        let west = Timestamp::parse("2026-10-28 12:00:00 -0500").unwrap();
        assert_eq!(west.to_string(), "2026-10-28 12:00:00 -0500");

        let utc = Timestamp::parse("2026-10-28 12:00:00 +0000").unwrap();
        assert_eq!(utc.to_string(), "2026-10-28 12:00:00 +0000");
    }

    #[test]
    fn colon_offsets_are_normalized() {
        let timestamp = Timestamp::parse("2026-10-28 12:00:00 +05:30").unwrap();
        assert_eq!(timestamp.to_string(), "2026-10-28 12:00:00 +0530");
    }

    #[test]
    fn ordering_compares_instants() {
        let kst = Timestamp::parse("2026-10-28 12:00:00 +0900").unwrap();
        let utc = Timestamp::parse("2026-10-28 12:00:00 +0000").unwrap();
        assert!(kst < utc);
    }

    #[test]
    fn invalid_timestamps_are_rejected() {
        assert!(Timestamp::parse("forever").is_err());
        assert!(Timestamp::parse("2026-10-28 12:00:00").is_err());
        assert!(Timestamp::parse("2026-10-28").is_err());
        assert!(Timestamp::parse("").is_err());
    }

    #[test]
    fn from_str_uses_the_same_parser() {
        let timestamp: Timestamp = "2026-10-28 12:00:00 +0900".parse().unwrap();
        assert_eq!(timestamp.to_string(), "2026-10-28 12:00:00 +0900");
    }
}
