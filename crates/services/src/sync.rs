//! Wrappers for synchronization containers.

use core::ops::Deref;

/// Alias for `Arc<T>`
pub type Shared<T> = std::sync::Arc<T>;

/// A mutex that can safely be in async contexts and avoids deadlocks.
#[derive(Default, Debug)]
pub struct SharedMutex<T>(Shared<parking_lot::Mutex<T>>);

impl<T> Clone for SharedMutex<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> Deref for SharedMutex<T> {
    type Target = Shared<parking_lot::Mutex<T>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> SharedMutex<T> {
    /// Creates a new `SharedMutex` with the given value.
    pub fn new(t: T) -> Self {
        Self(Shared::new(parking_lot::Mutex::new(t)))
    }

    /// Apply a function to the inner value and return a value.
    pub fn apply<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let mut t = self.0.lock();
        f(&mut t)
    }
}

impl<T> From<T> for SharedMutex<T> {
    fn from(t: T) -> Self {
        Self::new(t)
    }
}
