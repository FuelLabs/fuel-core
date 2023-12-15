use std::fmt::Debug;
use std::result::Result as StdResult;

use slog::*;
use slog_async;

/// An error that could come from either of two slog `Drain`s.
#[derive(Debug)]
enum SplitDrainError<D1, D2>
where
    D1: Drain,
    D2: Drain,
{
    Drain1Error(D1::Err),
    Drain2Error(D2::Err),
}

/// An slog `Drain` that forwards log messages to two other drains.
struct SplitDrain<D1, D2>
where
    D1: Drain,
    D2: Drain,
{
    drain1: D1,
    drain2: D2,
}

impl<D1, D2> SplitDrain<D1, D2>
where
    D1: Drain,
    D2: Drain,
{
    /// Creates a new split drain that forwards to the two provided drains.
    fn new(drain1: D1, drain2: D2) -> Self {
        SplitDrain { drain1, drain2 }
    }
}

impl<D1, D2> Drain for SplitDrain<D1, D2>
where
    D1: Drain,
    D2: Drain,
{
    type Ok = ();
    type Err = SplitDrainError<D1, D2>;

    fn log(&self, record: &Record, values: &OwnedKVList) -> StdResult<Self::Ok, Self::Err> {
        self.drain1
            .log(record, values)
            .map_err(SplitDrainError::Drain1Error)
            .and(
                self.drain2
                    .log(record, values)
                    .map(|_| ())
                    .map_err(SplitDrainError::Drain2Error),
            )
    }
}

/// Creates an async slog logger that writes to two drains in parallel.
pub fn split_logger<D1, D2>(drain1: D1, drain2: D2) -> Logger
where
    D1: Drain + Send + Debug + 'static,
    D2: Drain + Send + Debug + 'static,
    D1::Err: Debug,
    D2::Err: Debug,
{
    let split_drain = SplitDrain::new(drain1.fuse(), drain2.fuse()).fuse();
    let async_drain = slog_async::Async::new(split_drain)
        .chan_size(20000)
        .build()
        .fuse();
    Logger::root(async_drain, o!())
}
