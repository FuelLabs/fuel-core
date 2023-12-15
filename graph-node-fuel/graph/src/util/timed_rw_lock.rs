use parking_lot::{Mutex, RwLock};
use slog::{warn, Logger};
use std::time::{Duration, Instant};

use crate::prelude::ENV_VARS;

/// Adds instrumentation for timing the performance of the lock.
pub struct TimedRwLock<T> {
    id: String,
    lock: RwLock<T>,
    log_threshold: Duration,
}

impl<T> TimedRwLock<T> {
    pub fn new(x: T, id: impl Into<String>) -> Self {
        TimedRwLock {
            id: id.into(),
            lock: RwLock::new(x),
            log_threshold: ENV_VARS.lock_contention_log_threshold,
        }
    }

    pub fn write(&self, logger: &Logger) -> parking_lot::RwLockWriteGuard<T> {
        loop {
            let mut elapsed = Duration::from_secs(0);
            match self.lock.try_write_for(self.log_threshold) {
                Some(guard) => break guard,
                None => {
                    elapsed += self.log_threshold;
                    warn!(logger, "Write lock taking a long time to acquire";
                                  "id" => &self.id,
                                  "wait_ms" => elapsed.as_millis(),
                    );
                }
            }
        }
    }

    pub fn read(&self, logger: &Logger) -> parking_lot::RwLockReadGuard<T> {
        loop {
            let mut elapsed = Duration::from_secs(0);
            match self.lock.try_read_for(self.log_threshold) {
                Some(guard) => break guard,
                None => {
                    elapsed += self.log_threshold;
                    warn!(logger, "Read lock taking a long time to acquire";
                                  "id" => &self.id,
                                  "wait_ms" => elapsed.as_millis(),
                    );
                }
            }
        }
    }
}

/// Adds instrumentation for timing the performance of the lock.
pub struct TimedMutex<T> {
    id: String,
    lock: Mutex<T>,
    log_threshold: Duration,
}

impl<T> TimedMutex<T> {
    pub fn new(x: T, id: impl Into<String>) -> Self {
        TimedMutex {
            id: id.into(),
            lock: Mutex::new(x),
            log_threshold: ENV_VARS.lock_contention_log_threshold,
        }
    }

    pub fn lock(&self, logger: &Logger) -> parking_lot::MutexGuard<T> {
        let start = Instant::now();
        let guard = self.lock.lock();
        let elapsed = start.elapsed();
        if elapsed > self.log_threshold {
            warn!(logger, "Mutex lock took a long time to acquire";
                          "id" => &self.id,
                          "wait_ms" => elapsed.as_millis(),
            );
        }
        guard
    }
}
