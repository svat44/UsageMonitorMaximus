//! Injectable clock so time-dependent logic is deterministic in tests.

use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Manually advanced clock for tests.
#[derive(Debug)]
pub struct ManualClock(Mutex<DateTime<Utc>>);

impl ManualClock {
    pub fn new(start: DateTime<Utc>) -> Self {
        Self(Mutex::new(start))
    }
    pub fn advance(&self, by: Duration) {
        *self.0.lock().unwrap() += by;
    }
    pub fn set(&self, t: DateTime<Utc>) {
        *self.0.lock().unwrap() = t;
    }
}

impl Clock for ManualClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}
