//! Common traits and logic for managing the lifecycle of services
#![deny(unused_crate_dependencies)]
#![deny(missing_docs)]

mod service;

/// Re-exports for streaming utilities
pub mod stream {
    #[doc(no_inline)]
    pub use futures::stream::{
        pending,
        unfold,
        Stream,
    };

    /// A Send + Sync BoxStream
    pub type BoxStream<T> =
        core::pin::Pin<Box<dyn Stream<Item = T> + Send + Sync + 'static>>;
}

pub use service::{
    EmptyShared,
    RunnableService,
    RunnableTask,
    Service,
    ServiceRunner,
    Shared,
    State,
    StateWatcher,
};
