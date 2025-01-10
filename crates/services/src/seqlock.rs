//! A simple implementation of a sequential lock.
//! More details: <https://docs.kernel.org/locking/seqlock.html>

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

/// The writer handle for the `SeqLock`.
/// Only one writer exists for a `SeqLock`.
#[derive(Debug)]
pub struct SeqLockWriter<T> {
    lock: std::sync::Arc<SeqLock<T>>,
}

impl<T> SeqLockWriter<T> {
    /// Modifies the data within the lock.
    pub fn write<F>(&self, f: F)
    where
        F: FnOnce(&mut T),
    {
        let lock = &self.lock;

        // Indicate that a write operation is starting.
        lock.sequence.fetch_add(1, Ordering::Release);

        // Modify the data.
        unsafe {
            f(&mut *lock.data.get());
        }

        // Indicate that the write operation has finished.
        lock.sequence.fetch_add(1, Ordering::Release);
    }
}

/// The reader handle for the `SeqLock`.
/// Multiple readers can be created for a `SeqLock`.
#[derive(Clone, Debug)]
pub struct SeqLockReader<T> {
    lock: std::sync::Arc<SeqLock<T>>,
}

impl<T> SeqLockReader<T> {
    /// Reads the data within the lock.
    pub fn read(&self) -> T
    where
        T: Copy,
    {
        let lock = &self.lock;

        loop {
            // check starting guard
            let start = lock.sequence.load(Ordering::Acquire);

            // if odd, write in progress
            if start % 2 != 0 {
                continue;
            }

            let data = unsafe { *lock.data.get() };

            // check starting/ending guard
            let end = lock.sequence.load(Ordering::Acquire);

            // if value changed, retry
            if start == end && start % 2 == 0 {
                return data;
            }
        }
    }
}

impl<T> SeqLock<T> {
    /// Creates a new `SeqLock` and returns a writer and a reader handle.
    /// Optimized for occasional writes and frequent reads
    ///  !!WARNING!!
    /// ONLY USE IF ALL THE BELOW CRITERIA ARE MET
    ///  1. Internal data < 64 bytes
    ///  2. Data is Copy
    ///  3. ONLY 1 writer
    ///  4. VERY frequent reads
    #[allow(clippy::new_ret_no_self)]
    pub fn new(data: T) -> (SeqLockWriter<T>, SeqLockReader<T>) {
        let lock = Self {
            sequence: AtomicU64::new(0),
            data: UnsafeCell::new(data),
        };
        let shared = std::sync::Arc::new(lock);
        (
            SeqLockWriter {
                lock: std::sync::Arc::clone(&shared),
            },
            SeqLockReader { lock: shared },
        )
    }
}

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_seqlock__provides_correct_values_in_order() {
        let (writer, reader) = SeqLock::new(42);
        let iterations = 100;

        let writer = {
            thread::spawn(move || {
                for i in 0..iterations {
                    writer.write(|data| *data = i);
                }
            })
        };

        let reader = {
            let lock = reader.clone();
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
        let (writer, reader) = SeqLock::new(42);

        writer.write(|data| {
            *data = 100;
        });

        let value = reader.read();
        assert_eq!(value, 100);
    }
}
