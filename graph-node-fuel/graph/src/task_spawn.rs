//! The functions in this module should be used to execute futures, serving as a facade to the
//! underlying executor implementation which currently is tokio. This serves a few purposes:
//! - Avoid depending directly on tokio APIs, making upgrades or a potential switch easier.
//! - Reflect our chosen default semantics of aborting on task panic, offering `*_allow_panic`
//!   functions to opt out of that.
//! - Reflect that historically we've used blocking futures due to making DB calls directly within
//!   futures. This point should go away once https://github.com/graphprotocol/graph-node/issues/905
//!   is resolved. Then the blocking flavors should no longer accept futures but closures.
//!
//! These should not be called from within executors other than tokio, particularly the blocking
//! functions will panic in that case. We should generally avoid mixing executors whenever possible.

use futures03::future::{FutureExt, TryFutureExt};
use std::future::Future as Future03;
use std::panic::AssertUnwindSafe;
use tokio::task::JoinHandle;

fn abort_on_panic<T: Send + 'static>(
    f: impl Future03<Output = T> + Send + 'static,
) -> impl Future03<Output = T> {
    // We're crashing, unwind safety doesn't matter.
    AssertUnwindSafe(f).catch_unwind().unwrap_or_else(|_| {
        println!("Panic in tokio task, aborting!");
        std::process::abort()
    })
}

/// Aborts on panic.
pub fn spawn<T: Send + 'static>(f: impl Future03<Output = T> + Send + 'static) -> JoinHandle<T> {
    tokio::spawn(abort_on_panic(f))
}

pub fn spawn_allow_panic<T: Send + 'static>(
    f: impl Future03<Output = T> + Send + 'static,
) -> JoinHandle<T> {
    tokio::spawn(f)
}

/// Aborts on panic.
pub fn spawn_blocking<T: Send + 'static>(
    f: impl Future03<Output = T> + Send + 'static,
) -> JoinHandle<T> {
    tokio::task::spawn_blocking(move || block_on(abort_on_panic(f)))
}

/// Does not abort on panic, panics result in an `Err` in `JoinHandle`.
pub fn spawn_blocking_allow_panic<R: 'static + Send>(
    f: impl 'static + FnOnce() -> R + Send,
) -> JoinHandle<R> {
    tokio::task::spawn_blocking(f)
}

/// Runs the future on the current thread. Panics if not within a tokio runtime.
pub fn block_on<T>(f: impl Future03<Output = T>) -> T {
    tokio::runtime::Handle::current().block_on(f)
}

/// Spawns a thread with access to the tokio runtime. Panics if the thread cannot be spawned.
pub fn spawn_thread(
    name: impl Into<String>,
    f: impl 'static + FnOnce() + Send,
) -> std::thread::JoinHandle<()> {
    let conf = std::thread::Builder::new().name(name.into());
    let runtime = tokio::runtime::Handle::current();
    conf.spawn(move || {
        let _runtime_guard = runtime.enter();
        f()
    })
    .unwrap()
}
