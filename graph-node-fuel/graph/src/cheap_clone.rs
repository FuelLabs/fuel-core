use slog::Logger;
use std::future::Future;
use std::rc::Rc;
use std::sync::Arc;
use tonic::transport::Channel;

/// Things that are fast to clone in the context of an application such as Graph Node
///
/// The purpose of this API is to reduce the number of calls to .clone() which need to
/// be audited for performance.
///
/// As a rule of thumb, only constant-time Clone impls should also implement CheapClone.
/// Eg:
///    ✔ Arc<T>
///    ✗ Vec<T>
///    ✔ u128
///    ✗ String
pub trait CheapClone: Clone {
    #[inline]
    fn cheap_clone(&self) -> Self {
        self.clone()
    }
}

impl<T: ?Sized> CheapClone for Rc<T> {}
impl<T: ?Sized> CheapClone for Arc<T> {}
impl<T: ?Sized + CheapClone> CheapClone for Box<T> {}
impl<T: ?Sized + CheapClone> CheapClone for std::pin::Pin<T> {}
impl<T: CheapClone> CheapClone for Option<T> {}
impl CheapClone for Logger {}
// reqwest::Client uses Arc internally, so it is CheapClone.
impl CheapClone for reqwest::Client {}

// Pool is implemented as a newtype over Arc,
// So it is CheapClone.
impl<M: diesel::r2d2::ManageConnection> CheapClone for diesel::r2d2::Pool<M> {}

impl<F: Future> CheapClone for futures03::future::Shared<F> {}

impl CheapClone for Channel {}
