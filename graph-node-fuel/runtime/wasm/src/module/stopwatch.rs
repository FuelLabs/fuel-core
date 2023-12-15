// Copied from https://github.com/ellisonch/rust-stopwatch
// Copyright (c) 2014 Chucky Ellison <cme at freefour.com> under MIT license

use std::default::Default;
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub struct TimeoutStopwatch {
    /// The time the stopwatch was started last, if ever.
    start_time: Option<Instant>,
    /// The time elapsed while the stopwatch was running (between start() and stop()).
    pub elapsed: Duration,
}

impl Default for TimeoutStopwatch {
    fn default() -> TimeoutStopwatch {
        TimeoutStopwatch {
            start_time: None,
            elapsed: Duration::from_secs(0),
        }
    }
}

impl TimeoutStopwatch {
    /// Returns a new stopwatch.
    pub fn new() -> TimeoutStopwatch {
        let sw: TimeoutStopwatch = Default::default();
        sw
    }

    /// Returns a new stopwatch which will immediately be started.
    pub fn start_new() -> TimeoutStopwatch {
        let mut sw = TimeoutStopwatch::new();
        sw.start();
        sw
    }

    /// Starts the stopwatch.
    pub fn start(&mut self) {
        self.start_time = Some(Instant::now());
    }

    /// Stops the stopwatch.
    pub fn stop(&mut self) {
        self.elapsed = self.elapsed();
        self.start_time = None;
    }

    /// Returns the elapsed time since the start of the stopwatch.
    pub fn elapsed(&self) -> Duration {
        match self.start_time {
            // stopwatch is running
            Some(t1) => t1.elapsed() + self.elapsed,
            // stopwatch is not running
            None => self.elapsed,
        }
    }
}
