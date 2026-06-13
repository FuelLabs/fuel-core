//! Provides executors for spawning background tasks.
use futures::executor::ThreadPool;
use std::{future::Future, pin::Pin};

/// Implemented on objects that can run a `Future` in the background.
///
/// > **Note**: While it may be tempting to implement this trait on types such as
/// >           [`futures::stream::FuturesUnordered`], please note that passing an `Executor` is
/// >           optional, and that `FuturesUnordered` (or a similar struct) will automatically
/// >           be used as fallback by libp2p. The `Executor` trait should therefore only be
/// >           about running `Future`s on a separate task.
pub trait Executor {
    /// Run the given future in the background until it ends.
    #[track_caller]
    fn exec(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>);
}

impl<F: Fn(Pin<Box<dyn Future<Output = ()> + Send>>)> Executor for F {
    fn exec(&self, f: Pin<Box<dyn Future<Output = ()> + Send>>) {
        self(f)
    }
}

impl Executor for ThreadPool {
    fn exec(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        self.spawn_ok(future)
    }
}

#[cfg(all(
    feature = "tokio",
    not(any(target_os = "emscripten", target_os = "wasi", target_os = "unknown"))
))]
#[derive(Default, Debug, Clone, Copy)]
pub(crate) struct TokioExecutor;
#[cfg(all(
    feature = "tokio",
    not(any(target_os = "emscripten", target_os = "wasi", target_os = "unknown"))
))]
impl Executor for TokioExecutor {
    fn exec(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        tokio::spawn(future);
    }
}

#[cfg(all(
    feature = "async-std",
    not(any(target_os = "emscripten", target_os = "wasi", target_os = "unknown"))
))]
#[derive(Default, Debug, Clone, Copy)]
pub(crate) struct AsyncStdExecutor;
#[cfg(all(
    feature = "async-std",
    not(any(target_os = "emscripten", target_os = "wasi", target_os = "unknown"))
))]
impl Executor for AsyncStdExecutor {
    fn exec(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        async_std::task::spawn(future);
    }
}

#[cfg(feature = "wasm-bindgen")]
#[derive(Default, Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct WasmBindgenExecutor;
#[cfg(feature = "wasm-bindgen")]
impl Executor for WasmBindgenExecutor {
    fn exec(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        wasm_bindgen_futures::spawn_local(future)
    }
}
