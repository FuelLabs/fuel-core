use crate::ext::futures::FutureExtension;
use futures03::{Future, FutureExt, TryFutureExt};
use slog::{debug, trace, warn, Logger};
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use tokio_retry::Retry;

pub const RETRY_DEFAULT_LIMIT: Duration = Duration::from_secs(30);

/// Generic helper function for retrying async operations with built-in logging.
///
/// To use this helper, do the following:
///
/// 1. Call this function with an operation name (used for logging) and a `Logger`.
/// 2. Optional: Chain a call to `.when(...)` to set a custom retry condition.
/// 3. Optional: call `.log_after(...)` or `.no_logging()`.
/// 4. Call either `.limit(...)` or `.no_limit()`.
/// 5. Call one of `.timeout_secs(...)`, `.timeout_millis(...)`, `.timeout(...)`, and
///    `.no_timeout()`.
/// 6. Call `.run(...)`.
///
/// All steps are required, except Step 2 and Step 3.
///
/// Example usage:
/// ```
/// # extern crate graph;
/// # use graph::prelude::*;
/// # use tokio::time::timeout;
/// use std::future::Future;
/// use graph::prelude::{Logger, TimeoutError};
/// #
/// # type Memes = (); // the memes are a lie :(
/// #
/// # async fn  download_the_memes() -> Result<Memes, ()> {
/// #     Ok(())
/// # }
///
/// fn async_function(logger: Logger) -> impl Future<Output=Result<Memes, TimeoutError<()>>> {
///     // Retry on error
///     retry("download memes", &logger)
///         .no_limit() // Retry forever
///         .timeout_secs(30) // Retry if an attempt takes > 30 seconds
///         .run(|| {
///             download_the_memes() // Return a Future
///         })
/// }
/// ```
pub fn retry<I, E>(operation_name: impl ToString, logger: &Logger) -> RetryConfig<I, E> {
    RetryConfig {
        operation_name: operation_name.to_string(),
        logger: logger.clone(),
        condition: RetryIf::Error,
        log_after: 1,
        warn_after: 10,
        limit: RetryConfigProperty::Unknown,
        phantom_item: PhantomData,
        phantom_error: PhantomData,
    }
}

pub struct RetryConfig<I, E> {
    operation_name: String,
    logger: Logger,
    condition: RetryIf<I, E>,
    log_after: u64,
    warn_after: u64,
    limit: RetryConfigProperty<usize>,
    phantom_item: PhantomData<I>,
    phantom_error: PhantomData<E>,
}

impl<I, E> RetryConfig<I, E>
where
    I: Send,
    E: Debug + Send + Send + Sync + 'static,
{
    /// Sets a function used to determine if a retry is needed.
    /// Note: timeouts always trigger a retry.
    ///
    /// Overrides the default behaviour of retrying on any `Err`.
    pub fn when<P>(mut self, predicate: P) -> Self
    where
        P: Fn(&Result<I, E>) -> bool + Send + Sync + 'static,
    {
        self.condition = RetryIf::Predicate(Box::new(predicate));
        self
    }

    /// Only log retries after `min_attempts` failed attempts.
    pub fn log_after(mut self, min_attempts: u64) -> Self {
        self.log_after = min_attempts;
        self
    }

    pub fn warn_after(mut self, min_attempts: u64) -> Self {
        self.warn_after = min_attempts;
        self
    }

    /// Never log failed attempts.
    /// May still log at `trace` logging level.
    pub fn no_logging(mut self) -> Self {
        self.log_after = u64::max_value();
        self.warn_after = u64::max_value();
        self
    }

    /// Set a limit on how many retry attempts to make.
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit.set(limit);
        self
    }

    /// Allow unlimited retry attempts.
    pub fn no_limit(mut self) -> Self {
        self.limit.clear();
        self
    }

    /// Set how long (in seconds) to wait for an attempt to complete before giving up on that
    /// attempt.
    pub fn timeout_secs(self, timeout_secs: u64) -> RetryConfigWithTimeout<I, E> {
        self.timeout(Duration::from_secs(timeout_secs))
    }

    /// Set how long (in milliseconds) to wait for an attempt to complete before giving up on that
    /// attempt.
    pub fn timeout_millis(self, timeout_ms: u64) -> RetryConfigWithTimeout<I, E> {
        self.timeout(Duration::from_millis(timeout_ms))
    }

    /// Set how long to wait for an attempt to complete before giving up on that attempt.
    pub fn timeout(self, timeout: Duration) -> RetryConfigWithTimeout<I, E> {
        RetryConfigWithTimeout {
            inner: self,
            timeout,
        }
    }

    /// Allow attempts to take as long as they need (or potentially hang forever).
    pub fn no_timeout(self) -> RetryConfigNoTimeout<I, E> {
        RetryConfigNoTimeout { inner: self }
    }
}

pub struct RetryConfigWithTimeout<I, E> {
    inner: RetryConfig<I, E>,
    timeout: Duration,
}

impl<I, E> RetryConfigWithTimeout<I, E>
where
    I: Debug + Send + 'static,
    E: Debug + Send + Send + Sync + 'static,
{
    /// Rerun the provided function as many times as needed.
    pub fn run<F, R>(self, mut try_it: F) -> impl Future<Output = Result<I, TimeoutError<E>>>
    where
        F: FnMut() -> R + Send + 'static,
        R: Future<Output = Result<I, E>> + Send + 'static,
    {
        let operation_name = self.inner.operation_name;
        let logger = self.inner.logger.clone();
        let condition = self.inner.condition;
        let log_after = self.inner.log_after;
        let warn_after = self.inner.warn_after;
        let limit_opt = self.inner.limit.unwrap(&operation_name, "limit");
        let timeout = self.timeout;

        trace!(logger, "Run with retry: {}", operation_name);

        run_retry(
            operation_name,
            logger,
            condition,
            log_after,
            warn_after,
            limit_opt,
            move || {
                try_it()
                    .timeout(timeout)
                    .map_err(|_| TimeoutError::Elapsed)
                    .and_then(|res| std::future::ready(res.map_err(TimeoutError::Inner)))
                    .boxed()
            },
        )
    }
}

pub struct RetryConfigNoTimeout<I, E> {
    inner: RetryConfig<I, E>,
}

impl<I, E> RetryConfigNoTimeout<I, E> {
    /// Rerun the provided function as many times as needed.
    pub fn run<F, R>(self, try_it: F) -> impl Future<Output = Result<I, E>>
    where
        I: Debug + Send + 'static,
        E: Debug + Send + Sync + 'static,
        F: Fn() -> R + Send + 'static,
        R: Future<Output = Result<I, E>> + Send,
    {
        let operation_name = self.inner.operation_name;
        let logger = self.inner.logger.clone();
        let condition = self.inner.condition;
        let log_after = self.inner.log_after;
        let warn_after = self.inner.warn_after;
        let limit_opt = self.inner.limit.unwrap(&operation_name, "limit");

        trace!(logger, "Run with retry: {}", operation_name);

        run_retry(
            operation_name,
            logger,
            condition,
            log_after,
            warn_after,
            limit_opt,
            // No timeout, so all errors are inner errors
            move || try_it().map_err(TimeoutError::Inner),
        )
        .map_err(|e| {
            // No timeout, so all errors are inner errors
            e.into_inner().unwrap()
        })
    }
}

#[derive(Error, Debug)]
pub enum TimeoutError<T: Debug + Send + Sync + 'static> {
    #[error("{0:?}")]
    Inner(T),
    #[error("Timeout elapsed")]
    Elapsed,
}

impl<T: Debug + Send + Sync + 'static> TimeoutError<T> {
    pub fn is_elapsed(&self) -> bool {
        match self {
            TimeoutError::Inner(_) => false,
            TimeoutError::Elapsed => true,
        }
    }

    pub fn into_inner(self) -> Option<T> {
        match self {
            TimeoutError::Inner(x) => Some(x),
            TimeoutError::Elapsed => None,
        }
    }
}

fn run_retry<O, E, F, R>(
    operation_name: String,
    logger: Logger,
    condition: RetryIf<O, E>,
    log_after: u64,
    warn_after: u64,
    limit_opt: Option<usize>,
    mut try_it_with_timeout: F,
) -> impl Future<Output = Result<O, TimeoutError<E>>> + Send
where
    O: Debug + Send + 'static,
    E: Debug + Send + Sync + 'static,
    F: FnMut() -> R + Send + 'static,
    R: Future<Output = Result<O, TimeoutError<E>>> + Send,
{
    let condition = Arc::new(condition);

    let mut attempt_count = 0;

    Retry::spawn(retry_strategy(limit_opt, RETRY_DEFAULT_LIMIT), move || {
        let operation_name = operation_name.clone();
        let logger = logger.clone();
        let condition = condition.clone();

        attempt_count += 1;

        try_it_with_timeout().then(move |result_with_timeout| {
            let is_elapsed = result_with_timeout
                .as_ref()
                .err()
                .map(TimeoutError::is_elapsed)
                .unwrap_or(false);

            if is_elapsed {
                if attempt_count >= log_after {
                    debug!(
                        logger,
                        "Trying again after {} timed out (attempt #{})",
                        &operation_name,
                        attempt_count,
                    );
                }

                // Wrap in Err to force retry
                std::future::ready(Err(result_with_timeout))
            } else {
                // Any error must now be an inner error.
                // Unwrap the inner error so that the predicate doesn't need to think
                // about timeout::Error.
                let result = result_with_timeout.map_err(|e| e.into_inner().unwrap());

                // If needs retry
                if condition.check(&result) {
                    if attempt_count >= warn_after {
                        // This looks like it would be nice to de-duplicate, but if we try
                        // to use log! slog complains about requiring a const for the log level
                        // See also b05e1594-e408-4047-aefb-71fc60d70e8f
                        warn!(
                            logger,
                            "Trying again after {} failed (attempt #{}) with result {:?}",
                            &operation_name,
                            attempt_count,
                            result
                        );
                    } else if attempt_count >= log_after {
                        // See also b05e1594-e408-4047-aefb-71fc60d70e8f
                        debug!(
                            logger,
                            "Trying again after {} failed (attempt #{}) with result {:?}",
                            &operation_name,
                            attempt_count,
                            result
                        );
                    }

                    // Wrap in Err to force retry
                    std::future::ready(Err(result.map_err(TimeoutError::Inner)))
                } else {
                    // Wrap in Ok to prevent retry
                    std::future::ready(Ok(result.map_err(TimeoutError::Inner)))
                }
            }
        })
    })
    .boxed()
    .then(|retry_result| async {
        // Unwrap the inner result.
        // The outer Ok/Err is only used for retry control flow.
        match retry_result {
            Ok(r) => r,
            Err(e) => e,
        }
    })
}

pub fn retry_strategy(
    limit_opt: Option<usize>,
    max_delay: Duration,
) -> Box<dyn Iterator<Item = Duration> + Send> {
    // Exponential backoff, but with a maximum
    let backoff = ExponentialBackoff::from_millis(10)
        .max_delay(Duration::from_millis(
            // This should be fine, if the value is too high it will crash during
            // testing.
            max_delay.as_millis().try_into().unwrap(),
        ))
        .map(jitter);

    // Apply limit (maximum retry count)
    match limit_opt {
        Some(limit) => {
            // Items are delays *between* attempts,
            // so subtract 1 from limit.
            Box::new(backoff.take(limit - 1))
        }
        None => Box::new(backoff),
    }
}

enum RetryIf<I, E> {
    Error,
    Predicate(Box<dyn Fn(&Result<I, E>) -> bool + Send + Sync>),
}

impl<I, E> RetryIf<I, E> {
    fn check(&self, result: &Result<I, E>) -> bool {
        match *self {
            RetryIf::Error => result.is_err(),
            RetryIf::Predicate(ref pred) => pred(result),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RetryConfigProperty<V> {
    /// Property was explicitly set
    Set(V),

    /// Property was explicitly unset
    Clear,

    /// Property was not explicitly set or unset
    Unknown,
}

impl<V> RetryConfigProperty<V>
where
    V: PartialEq + Eq,
{
    fn set(&mut self, v: V) {
        if *self != RetryConfigProperty::Unknown {
            panic!("Retry config properties must be configured only once");
        }

        *self = RetryConfigProperty::Set(v);
    }

    fn clear(&mut self) {
        if *self != RetryConfigProperty::Unknown {
            panic!("Retry config properties must be configured only once");
        }

        *self = RetryConfigProperty::Clear;
    }

    fn unwrap(self, operation_name: &str, property_name: &str) -> Option<V> {
        match self {
            RetryConfigProperty::Set(v) => Some(v),
            RetryConfigProperty::Clear => None,
            RetryConfigProperty::Unknown => panic!(
                "Retry helper for {} must have {} parameter configured",
                operation_name, property_name
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures::future;
    use futures03::compat::Future01CompatExt;
    use slog::o;
    use std::sync::Mutex;

    #[tokio::test]
    async fn test() {
        let logger = Logger::root(::slog::Discard, o!());

        let result = {
            let c = Mutex::new(0);
            retry("test", &logger)
                .no_logging()
                .no_limit()
                .no_timeout()
                .run(move || {
                    let mut c_guard = c.lock().unwrap();
                    *c_guard += 1;

                    if *c_guard >= 10 {
                        future::ok(*c_guard).compat()
                    } else {
                        future::err(()).compat()
                    }
                })
                .await
        };
        assert_eq!(result, Ok(10));
    }

    #[tokio::test]
    async fn limit_reached() {
        let logger = Logger::root(::slog::Discard, o!());

        let result = {
            let c = Mutex::new(0);
            retry("test", &logger)
                .no_logging()
                .limit(5)
                .no_timeout()
                .run(move || {
                    let mut c_guard = c.lock().unwrap();
                    *c_guard += 1;

                    if *c_guard >= 10 {
                        future::ok(*c_guard).compat()
                    } else {
                        future::err(*c_guard).compat()
                    }
                })
                .await
        };
        assert_eq!(result, Err(5));
    }

    #[tokio::test]
    async fn limit_not_reached() {
        let logger = Logger::root(::slog::Discard, o!());

        let result = {
            let c = Mutex::new(0);
            retry("test", &logger)
                .no_logging()
                .limit(20)
                .no_timeout()
                .run(move || {
                    let mut c_guard = c.lock().unwrap();
                    *c_guard += 1;

                    if *c_guard >= 10 {
                        future::ok(*c_guard).compat()
                    } else {
                        future::err(*c_guard).compat()
                    }
                })
                .await
        };
        assert_eq!(result, Ok(10));
    }

    #[tokio::test]
    async fn custom_when() {
        let logger = Logger::root(::slog::Discard, o!());
        let c = Mutex::new(0);

        let result = {
            retry("test", &logger)
                .when(|result| result.unwrap() < 10)
                .no_logging()
                .limit(20)
                .no_timeout()
                .run(move || {
                    let mut c_guard = c.lock().unwrap();
                    *c_guard += 1;
                    if *c_guard > 30 {
                        future::err(()).compat()
                    } else {
                        future::ok(*c_guard).compat()
                    }
                })
                .await
        };

        assert_eq!(result, Ok(10));
    }
}
