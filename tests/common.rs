#![allow(unused_attributes)]
#![feature(once_cell)]

use std::lazy::SyncLazy;
use std::sync::Mutex;
use std::{mem, slice};

pub fn d<T: Default>() -> T {
    Default::default()
}

static mut R: SyncLazy<Mutex<u8>> = SyncLazy::new(|| Mutex::new(0));

/// Unsafe function that will return a default implementation filled with
/// pseudo-random bytes.
///
/// Should not be used with types with internal memory addresses such as Vec
///
/// Implementation quite prone to UB and should NEVER be used outside the tests
/// scope
pub fn r<T: Default>() -> T {
    let mut t = Default::default();
    let len = mem::size_of::<T>();

    let b = unsafe {
        R.lock()
            .map(|mut b| {
                let a = *b;
                *b = a.wrapping_add(1);
                a
            })
            .expect("Failed to read Mutex!")
    };

    let p = (&mut t as *mut T).cast::<u8>();
    unsafe { slice::from_raw_parts_mut(p, len) }
        .iter_mut()
        .for_each(|s| *s = b);

    t
}
