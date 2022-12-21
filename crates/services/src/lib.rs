#![deny(unused_crate_dependencies)]
// TODO: Add documentation
// #![deny(missing_docs)]

mod service;

pub use service::{
    empty_shared,
    EmptyShared,
    RunnableService,
    Service,
    ServiceRunner,
    Shared,
    State,
};
