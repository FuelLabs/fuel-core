//! A simple implementation of a sequential lock.
//! More details: <https://docs.kernel.org/locking/seqlock.html>

use std::{
    cell::UnsafeCell,
    panic::UnwindSafe,
    sync::atomic::{
        fence,
        AtomicU64,
        Ordering,
    },
};

/// A simple implementation of a sequential lock.
/// some usage of unsafe, T must be Copy
#[derive(Debug)]
pub struct SeqLock<T: Copy> {
    sequence: AtomicU64,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send + Copy> Sync for SeqLock<T> {}

/// The writer handle for the `SeqLock`.
/// Only one writer exists for a `SeqLock`.
/// There is no Clone bound since we want to enforce only one writer.
#[derive(Debug)]
pub struct SeqLockWriter<T: Copy> {
    lock: std::sync::Arc<SeqLock<T>>,
}

impl<T: Copy> SeqLockWriter<T> {
    /// Modifies the data within the lock.
    pub fn write<F>(&self, f: F)
    where
        F: FnOnce(&mut T) + UnwindSafe,
    {
        let lock = &self.lock;

        // Indicate that a write operation is starting.
        lock.sequence.fetch_add(1, Ordering::AcqRel);
        // reordering safety
        fence(Ordering::Acquire);

        // attempt to perform the write, and catch any panics
        // we won't have partial write problems since data <= 64 bytes
        // safety: panics are caught and resumed
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            let data = &mut *lock.data.get();
            f(data);
        }));

        // reordering safety
        fence(Ordering::Release);
        // Indicate that the write operation has finished.
        lock.sequence.fetch_add(1, Ordering::Release);

        // resume unwinding if there was an error
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }
}

/// The reader handle for the `SeqLock`.
/// Multiple readers can be created for a `SeqLock`.
#[derive(Clone, Debug)]
pub struct SeqLockReader<T: Copy> {
    lock: std::sync::Arc<SeqLock<T>>,
}

impl<T: Copy> SeqLockReader<T> {
    /// Reads the data within the lock.
    pub fn read(&self) -> T {
        let lock = &self.lock;

        loop {
            // check starting guard
            let start = lock.sequence.load(Ordering::Acquire);

            // if odd, write in progress
            if start % 2 != 0 {
                std::thread::yield_now();
                continue;
            }

            // reordering safety
            fence(Ordering::Acquire);

            // safety: when the data <=64 bytes, it fits in a single cache line
            // and cannot be subject to torn reads
            let data = unsafe { *lock.data.get() };

            // reordering safety
            fence(Ordering::Acquire);

            // check starting/ending guard
            let end = lock.sequence.load(Ordering::Acquire);

            // if value changed, retry
            if start == end && start % 2 == 0 {
                return data;
            }
        }
    }
}

impl<T: Copy> SeqLock<T> {
    /// Creates a new `SeqLock` and returns a writer and a reader handle.
    /// Optimized for occasional writes and frequent reads
    ///  !!WARNING!!
    /// ONLY USE IF ALL THE BELOW CRITERIA ARE MET
    ///  1. Internal data <= 64 bytes
    ///  2. VERY frequent reads
    /// # Safety
    /// The data must be `Copy`
    #[allow(clippy::new_ret_no_self)]
    pub unsafe fn new(data: T) -> (SeqLockWriter<T>, SeqLockReader<T>) {
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
        let (writer, reader) = unsafe { SeqLock::new(42) };
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
        let (writer, reader) = unsafe { SeqLock::new(42) };

        writer.write(|data| {
            *data = 100;
        });

        let value = reader.read();
        assert_eq!(value, 100);
    }
}
