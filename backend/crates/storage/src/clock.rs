//! The source of "now". Services never call `Utc::now()` directly, so tests can move time.

use std::sync::Arc;

use chrono::{DateTime, NaiveDate, Utc};

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

pub type SharedClock = Arc<dyn Clock>;

/// The calendar date of an instant, in UTC.
pub fn date_of(at: DateTime<Utc>) -> NaiveDate {
    at.date_naive()
}

/// Read an ISO-8601 timestamp such as `2026-08-21T06:41:00.000Z`.
pub fn parse_iso(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|at| at.with_timezone(&Utc))
}
