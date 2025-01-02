//! A simple implementation of a sequential lock.
//! More details: <https://docs.kernel.org/locking/seqlock.html>
//! Optimized for occasional writes and frequent reads

use std::{
    cell::UnsafeCell,
    sync::atomic::{
        AtomicU64,
        Ordering,
    },
};

/// A simple implementation of a sequential lock.
/// some usage of unsafe, T must be Copy
#[derive(Debug)]
pub struct SeqLock<T> {
    sequence: AtomicU64,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for SeqLock<T> {}

impl<T> SeqLock<T> {
    /// Create a new SeqLock with the given data
    pub fn new(data: T) -> Self {
        Self {
            sequence: AtomicU64::new(0),
            data: UnsafeCell::new(data),
        }
    }

    /// Write the data
    pub fn write<F>(&self, f: F)
    where
        F: FnOnce(&mut T),
    {
        // starting guard
        self.sequence.fetch_add(1, Ordering::Release);

        // Modify the data
        unsafe {
            f(&mut *self.data.get());
        }

        // ending guard
        self.sequence.fetch_add(1, Ordering::Release);
    }

    /// Read the data
    pub fn read(&self) -> T
    where
        T: Copy,
    {
        loop {
            // check starting guard
            let start = self.sequence.load(Ordering::Acquire);

            // if odd, write in progress
            if start % 2 != 0 {
                continue;
            }

            let data = unsafe { *self.data.get() };

            // check starting/ending guard
            let end = self.sequence.load(Ordering::Acquire);

            // if value changed, retry
            if start == end && start % 2 == 0 {
                return data;
            }
        }
    }
}

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::Arc,
        thread,
    };

    #[test]
    fn test_seqlock__provides_correct_values_in_order() {
        let lock = Arc::new(SeqLock::new(42));
        let iterations = 100;

        let writer = {
            let lock = lock.clone();
            thread::spawn(move || {
                for i in 0..iterations {
                    lock.write(|data| *data = i);
                }
            })
        };

        let reader = {
            let lock = lock.clone();
            thread::spawn(move || {
                let seen = 0;

                for _ in 0..iterations {
                    let value = lock.read();
                    assert!(value >= seen);
                }
            })
        };

        writer.join().unwrap();
        reader.join().unwrap();
    }

    #[test]
    fn test_seqlock__single_threaded() {
        let lock = SeqLock::new(42);

        lock.write(|data| {
            *data = 100;
        });

        let value = lock.read();
        assert_eq!(value, 100);
    }
}
