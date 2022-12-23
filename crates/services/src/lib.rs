#![deny(unused_crate_dependencies)]
// TODO: Add documentation
// #![deny(missing_docs)]

mod service;

pub type BoxStream<T> = core::pin::Pin<Box<dyn Stream<Item = T> + Send + Sync + 'static>>;

pub use futures::stream::{
    pending,
    unfold,
    Stream,
};
pub use service::{
    EmptyShared,
    RunnableService,
    Service,
    ServiceRunner,
    Shared,
    State,
};
