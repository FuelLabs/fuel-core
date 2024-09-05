mod config;
mod error;
mod pool;
mod registries;
mod service;

#[cfg(test)]
mod tests;

pub use service::{
    new_service,
    Service,
    SharedState,
};
