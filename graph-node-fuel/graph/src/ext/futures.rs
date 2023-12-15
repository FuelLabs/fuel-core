use crate::prelude::tokio::macros::support::Poll;
use crate::prelude::{Pin, StoreError};
use futures03::channel::oneshot;
use futures03::{future::Fuse, Future, FutureExt, Stream};
use std::fmt::{Debug, Display};
use std::sync::{Arc, Mutex, Weak};
use std::task::Context;
use std::time::Duration;

/// A cancelable stream or future.
///
/// Created by calling `cancelable` extension method.
/// Can be canceled through the corresponding `CancelGuard`.
pub struct Cancelable<T, C> {
    inner: T,
    cancel_receiver: Fuse<oneshot::Receiver<()>>,
    on_cancel: C,
}

impl<T, C> Cancelable<T, C> {
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

/// It's not viable to use `select` directly, so we do a custom implementation.
impl<S: Stream + Unpin, C: Fn() -> S::Item + Unpin> Stream for Cancelable<S, C> {
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Error if the stream was canceled by dropping the sender.
        match self.cancel_receiver.poll_unpin(cx) {
            Poll::Ready(Ok(_)) => unreachable!(),
            Poll::Ready(Err(_)) => Poll::Ready(Some((self.on_cancel)())),
            Poll::Pending => Pin::new(&mut self.inner).poll_next(cx),
        }
    }
}

impl<F: Future + Unpin, C: Fn() -> F::Output + Unpin> Future for Cancelable<F, C> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Error if the future was canceled by dropping the sender.
        // `canceled` is fused so we may ignore `Ok`s.
        match self.cancel_receiver.poll_unpin(cx) {
            Poll::Ready(Ok(_)) => unreachable!(),
            Poll::Ready(Err(_)) => Poll::Ready((self.on_cancel)()),
            Poll::Pending => Pin::new(&mut self.inner).poll(cx),
        }
    }
}

/// A `CancelGuard` or `SharedCancelGuard`.
pub trait Canceler {
    /// Adds `cancel_sender` to the set being guarded.
    /// Avoid calling directly and prefer using `cancelable`.
    fn add_cancel_sender(&self, cancel_sender: oneshot::Sender<()>);
}

/// Cancels any guarded futures and streams when dropped.
#[derive(Debug, Default)]
pub struct CancelGuard {
    /// This is the only non-temporary strong reference to this `Arc`, therefore
    /// the `Vec` should be dropped shortly after `self` is dropped.
    cancel_senders: Arc<Mutex<Vec<oneshot::Sender<()>>>>,
}

impl CancelGuard {
    /// Creates a guard that initially guards nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// A more readable `drop`.
    pub fn cancel(self) {}

    pub fn handle(&self) -> CancelHandle {
        CancelHandle {
            guard: Arc::downgrade(&self.cancel_senders),
        }
    }
}

impl Canceler for CancelGuard {
    fn add_cancel_sender(&self, cancel_sender: oneshot::Sender<()>) {
        self.cancel_senders.lock().unwrap().push(cancel_sender);
    }
}

/// A shared handle to a guard, used to add more cancelables. The handle
/// may outlive the guard, if `cancelable` is called with a handle to a
/// dropped guard, then the future or stream it is immediately canceled.
///
/// Dropping a handle has no effect.
#[derive(Clone, Debug)]
pub struct CancelHandle {
    guard: Weak<Mutex<Vec<oneshot::Sender<()>>>>,
}

pub trait CancelToken {
    fn is_canceled(&self) -> bool;
    fn check_cancel(&self) -> Result<(), Canceled> {
        if self.is_canceled() {
            Err(Canceled)
        } else {
            Ok(())
        }
    }
}

pub struct NeverCancel;

impl CancelToken for NeverCancel {
    #[inline]
    fn is_canceled(&self) -> bool {
        false
    }
}

pub struct Canceled;

impl CancelToken for CancelHandle {
    fn is_canceled(&self) -> bool {
        // Has been canceled if and only if the guard is gone.
        self.guard.upgrade().is_none()
    }
}

impl Canceler for CancelHandle {
    fn add_cancel_sender(&self, cancel_sender: oneshot::Sender<()>) {
        if let Some(guard) = self.guard.upgrade() {
            // If the guard exists, register the canceler.
            guard.lock().unwrap().push(cancel_sender);
        } else {
            // Otherwise cancel immediately.
            drop(cancel_sender)
        }
    }
}

/// A cancelation guard that can be canceled through a shared reference such as
/// an `Arc`.
///
/// To cancel guarded streams or futures, call `cancel` or drop the guard.
#[derive(Debug)]
pub struct SharedCancelGuard {
    guard: Mutex<Option<CancelGuard>>,
}

impl SharedCancelGuard {
    /// Creates a guard that initially guards nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Cancels the stream, a noop if already canceled.
    pub fn cancel(&self) {
        *self.guard.lock().unwrap() = None
    }

    pub fn is_canceled(&self) -> bool {
        self.guard.lock().unwrap().is_none()
    }

    pub fn handle(&self) -> CancelHandle {
        if let Some(ref guard) = *self.guard.lock().unwrap() {
            guard.handle()
        } else {
            // A handle that is always canceled.
            CancelHandle { guard: Weak::new() }
        }
    }
}

impl Default for SharedCancelGuard {
    fn default() -> Self {
        Self {
            guard: Mutex::new(Some(CancelGuard::new())),
        }
    }
}

impl Canceler for SharedCancelGuard {
    /// Cancels immediately if `self` has already been canceled.
    fn add_cancel_sender(&self, cancel_sender: oneshot::Sender<()>) {
        if let Some(ref mut guard) = *self.guard.lock().unwrap() {
            guard.add_cancel_sender(cancel_sender);
        } else {
            drop(cancel_sender)
        }
    }
}

/// An implementor of `Canceler` that never cancels,
/// making `cancelable` a noop.
#[derive(Debug, Default)]
pub struct DummyCancelGuard;

impl Canceler for DummyCancelGuard {
    fn add_cancel_sender(&self, cancel_sender: oneshot::Sender<()>) {
        // Send to the channel, preventing cancelation.
        let _ = cancel_sender.send(());
    }
}

pub trait StreamExtension: Stream + Sized {
    /// When `cancel` is called on a `CancelGuard` or it is dropped,
    /// `Cancelable` receives an error.
    ///
    fn cancelable<C: Fn() -> Self::Item>(
        self,
        guard: &impl Canceler,
        on_cancel: C,
    ) -> Cancelable<Self, C>;
}

impl<S: Stream> StreamExtension for S {
    fn cancelable<C: Fn() -> S::Item>(
        self,
        guard: &impl Canceler,
        on_cancel: C,
    ) -> Cancelable<Self, C> {
        let (canceler, cancel_receiver) = oneshot::channel();
        guard.add_cancel_sender(canceler);
        Cancelable {
            inner: self,
            cancel_receiver: cancel_receiver.fuse(),
            on_cancel,
        }
    }
}

pub trait FutureExtension: Future + Sized {
    /// When `cancel` is called on a `CancelGuard` or it is dropped,
    /// `Cancelable` receives an error.
    ///
    /// `on_cancel` is called to make an error value upon cancelation.
    fn cancelable<C: Fn() -> Self::Output>(
        self,
        guard: &impl Canceler,
        on_cancel: C,
    ) -> Cancelable<Self, C>;

    fn timeout(self, dur: Duration) -> tokio::time::Timeout<Self>;
}

impl<F: Future> FutureExtension for F {
    fn cancelable<C: Fn() -> F::Output>(
        self,
        guard: &impl Canceler,
        on_cancel: C,
    ) -> Cancelable<Self, C> {
        let (canceler, cancel_receiver) = oneshot::channel();
        guard.add_cancel_sender(canceler);
        Cancelable {
            inner: self,
            cancel_receiver: cancel_receiver.fuse(),
            on_cancel,
        }
    }

    fn timeout(self, dur: Duration) -> tokio::time::Timeout<Self> {
        tokio::time::timeout(dur, self)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum CancelableError<E: Display + Debug> {
    #[error("operation canceled")]
    Cancel,

    #[error("{0:}")]
    Error(E),
}

impl From<StoreError> for CancelableError<anyhow::Error> {
    fn from(e: StoreError) -> Self {
        Self::Error(anyhow::Error::from(e))
    }
}

impl From<CancelableError<anyhow::Error>> for CancelableError<StoreError> {
    fn from(e: CancelableError<anyhow::Error>) -> Self {
        match e {
            CancelableError::Error(e) => CancelableError::Error(e.into()),
            CancelableError::Cancel => CancelableError::Cancel,
        }
    }
}

impl From<StoreError> for CancelableError<StoreError> {
    fn from(e: StoreError) -> Self {
        Self::Error(e)
    }
}

impl<E: Display + Debug> From<Canceled> for CancelableError<E> {
    fn from(_: Canceled) -> Self {
        Self::Cancel
    }
}

impl From<diesel::result::Error> for CancelableError<anyhow::Error> {
    fn from(e: diesel::result::Error) -> Self {
        Self::Error(e.into())
    }
}

impl From<diesel::result::Error> for CancelableError<StoreError> {
    fn from(e: diesel::result::Error) -> Self {
        Self::Error(e.into())
    }
}

impl From<anyhow::Error> for CancelableError<anyhow::Error> {
    fn from(e: anyhow::Error) -> Self {
        Self::Error(e)
    }
}

impl From<CancelableError<StoreError>> for StoreError {
    fn from(err: CancelableError<StoreError>) -> StoreError {
        use CancelableError::*;
        match err {
            Cancel => StoreError::Canceled,
            Error(e) => e,
        }
    }
}
