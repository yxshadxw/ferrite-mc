//! Helpers shared by the integration tests.

use std::sync::{Arc, Mutex};

use ferrite_admin::{Clock, Timestamp};

/// A clock that tests move by hand.
pub struct TestClock {
    now: Mutex<Timestamp>,
}

impl TestClock {
    /// Starts the clock at `value`.
    pub fn new(value: &str) -> Arc<Self> {
        Arc::new(Self {
            now: Mutex::new(Timestamp::parse(value).expect("valid test clock timestamp")),
        })
    }

    /// Moves the clock to `value`.
    pub fn set(&self, value: &str) {
        *self.now.lock().expect("test clock") =
            Timestamp::parse(value).expect("valid test clock timestamp");
    }
}

impl Clock for TestClock {
    fn now(&self) -> Timestamp {
        *self.now.lock().expect("test clock")
    }
}
