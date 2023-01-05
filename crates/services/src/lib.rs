//! Common traits and logic for managing the lifecycle of services
#![deny(unused_crate_dependencies)]
#![deny(missing_docs)]

mod service;
mod state;

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

/// The source of some network data.
pub struct SourcePeer<T> {
    /// The source of the data.
    pub peer_id: PeerId,
    /// The data.
    pub data: T,
}

/// Placeholder for a peer id
pub struct PeerId;

pub use service::{
    EmptyShared,
    RunnableService,
    RunnableTask,
    Service,
    ServiceRunner,
    Shared,
    SharedMutex,
};
pub use state::{
    State,
    StateWatcher,
};
