#![deny(unused_crate_dependencies)]
// TODO: Add documentation
// #![deny(missing_docs)]

mod service;

pub mod stream {
    #[doc(no_inline)]
    pub use futures::stream::{
        pending,
        unfold,
        Stream,
    };
    pub type BoxStream<T> =
        core::pin::Pin<Box<dyn Stream<Item = T> + Send + Sync + 'static>>;
}

pub use service::{
    EmptyShared,
    RunnableService,
    Service,
    ServiceRunner,
    Shared,
    State,
};
