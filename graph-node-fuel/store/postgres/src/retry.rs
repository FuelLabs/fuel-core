//! Helpers to retry an operation indefinitely with exponential backoff
//! while the database is not available
use std::time::Duration;

use graph::{
    prelude::StoreError,
    slog::{warn, Logger},
    util::backoff::ExponentialBackoff,
};

const BACKOFF_BASE: Duration = Duration::from_millis(100);
const BACKOFF_CEIL: Duration = Duration::from_secs(10);

fn log_backoff_warning(logger: &Logger, op: &str, backoff: &ExponentialBackoff) {
    warn!(logger,
            "database unavailable, will retry";
            "operation" => op,
            "attempt" => backoff.attempt,
            "delay_ms" => backoff.delay().as_millis());
}

/// Run `f` with exponential backoff until it succeeds or it produces an
/// error other than `DatabaseUnavailable`. In other words, keep retrying
/// `f` until the database is available.
///
/// Do not use this from an async context since it will block the current
/// thread. Use `forever_async` instead
pub(crate) fn forever<T, F>(logger: &Logger, op: &str, f: F) -> Result<T, StoreError>
where
    F: Fn() -> Result<T, StoreError>,
{
    let mut backoff = ExponentialBackoff::new(BACKOFF_BASE, BACKOFF_CEIL);
    loop {
        match f() {
            Ok(v) => return Ok(v),
            Err(StoreError::DatabaseUnavailable) => {
                log_backoff_warning(logger, op, &backoff);
            }
            Err(e) => return Err(e),
        }
        backoff.sleep();
    }
}

/// Run `f` with exponential backoff until it succeeds or it produces an
/// error other than `DatabaseUnavailable`. In other words, keep retrying
/// `f` until the database is available.
pub(crate) async fn forever_async<T, F, Fut>(
    logger: &Logger,
    op: &str,
    f: F,
) -> Result<T, StoreError>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, StoreError>>,
{
    let mut backoff = ExponentialBackoff::new(BACKOFF_BASE, BACKOFF_CEIL);
    loop {
        match f().await {
            Ok(v) => return Ok(v),
            Err(StoreError::DatabaseUnavailable) => {
                log_backoff_warning(logger, op, &backoff);
            }
            Err(e) => return Err(e),
        }
        backoff.sleep_async().await;
    }
}
