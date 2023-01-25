#![deny(unused_crate_dependencies)]
#![deny(missing_docs)]
//! # Sync Service
//! Responsible for syncing the blockchain from the network.

pub mod import;
pub mod ports;
pub mod service;
mod state;
pub mod sync;

use std::fmt::Display;

pub use import::Config;

#[cfg(test)]
fuel_core_trace::enable_tracing!();

trait TraceErr {
    fn trace_err(self, msg: &str) -> Self;
}

trait TraceNone: Sized {
    fn trace_none<F>(self, f: F) -> Self
    where
        F: FnOnce();
    fn trace_none_error(self, msg: &str) -> Self {
        self.trace_none(|| tracing::error!("{}", msg))
    }
    fn trace_none_warn(self, msg: &str) -> Self {
        self.trace_none(|| tracing::warn!("{}", msg))
    }
    fn trace_none_info(self, msg: &str) -> Self {
        self.trace_none(|| tracing::info!("{}", msg))
    }
    fn trace_none_debug(self, msg: &str) -> Self {
        self.trace_none(|| tracing::debug!("{}", msg))
    }
    fn trace_none_trace(self, msg: &str) -> Self {
        self.trace_none(|| tracing::trace!("{}", msg))
    }
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

impl<T> TraceNone for Option<T> {
    fn trace_none<F>(self, f: F) -> Self
    where
        F: FnOnce(),
    {
        if self.is_none() {
            f();
        }
        self
    }
}

impl TraceNone for bool {
    fn trace_none<F>(self, f: F) -> Self
    where
        F: FnOnce(),
    {
        if !self {
            f();
        }
        self
    }
}
