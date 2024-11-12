//! Common traits and logic for managing the lifecycle of services
#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(missing_docs)]
#![deny(warnings)]

mod async_processor;
mod service;
mod state;
mod sync;
#[cfg(feature = "sync-processor")]
mod sync_processor;
pub mod yield_stream;

/// Re-exports for streaming utilities
pub mod stream {
    #[doc(no_inline)]
    pub use futures::stream::{
        pending,
        unfold,
        Stream,
    };

    /// A `Send` + `Sync` BoxStream with static lifetime.
    pub type BoxStream<T> =
        core::pin::Pin<Box<dyn Stream<Item = T> + Send + Sync + 'static>>;

    /// A `Send` BoxStream with a lifetime.
    pub type RefBoxStream<'a, T> = core::pin::Pin<Box<dyn Stream<Item = T> + Send + 'a>>;

    /// A Send + Sync BoxFuture
    pub type BoxFuture<'a, T> =
        core::pin::Pin<Box<dyn futures::Future<Output = T> + Send + Sync + 'a>>;

    /// Helper trait to create a BoxStream from a Stream
    pub trait IntoBoxStream: Stream {
        /// Convert this stream into a [`BoxStream`].
        fn into_boxed(self) -> BoxStream<Self::Item>
        where
            Self: Sized + Send + Sync + 'static,
        {
            Box::pin(self)
        }

        /// Convert this stream into a [`RefBoxStream`].
        fn into_boxed_ref<'a>(self) -> RefBoxStream<'a, Self::Item>
        where
            Self: Sized + Send + 'a,
        {
            Box::pin(self)
        }
    }

    impl<S> IntoBoxStream for S where S: Stream + Send {}
}

/// Helper trait to trace errors
pub trait TraceErr {
    /// Trace an error with a message.
    fn trace_err(self, msg: &str) -> Self;
}

impl<T, E> TraceErr for Result<T, E>
where
    E: Display,
{
    fn trace_err(self, msg: &str) -> Self {
        if let Err(e) = &self {
            tracing::error!("{} {}", msg, e);
        }
        self
    }
}

pub use async_processor::AsyncProcessor;
pub use service::{
    EmptyShared,
    RunnableService,
    RunnableTask,
    Service,
    ServiceRunner,
    TaskRunResult,
};
pub use state::{
    State,
    StateWatcher,
};
use std::fmt::Display;
pub use sync::{
    Shared,
    SharedMutex,
};
#[cfg(feature = "sync-processor")]
pub use sync_processor::SyncProcessor;

// For tests
use crate as fuel_core_services;
#[allow(unused_imports)]
use fuel_core_services as _;
