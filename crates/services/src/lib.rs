#![deny(unused_crate_dependencies)]
// TODO: Add documentation
// #![deny(missing_docs)]

mod service;

pub use service::{
    EmptyShared,
    RunnableService,
    Service,
    ServiceRunner,
    Shared,
    State,
};
