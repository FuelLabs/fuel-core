extern crate alloc;
use alloc::vec::Vec;

use fuel_indexer_schema::{
    deserialize, serialize, FtColumn, LOG_LEVEL_DEBUG, LOG_LEVEL_ERROR, LOG_LEVEL_INFO,
    LOG_LEVEL_TRACE, LOG_LEVEL_WARN,
};

pub mod types {
    pub use fuel_indexer_schema::*;
}

extern "C" {
    // TODO: error codes? or just panic and let the runtime handle it?
    fn ff_get_object(type_id: u64, ptr: *const u8, len: *mut u8) -> *mut u8;
    fn ff_put_object(type_id: u64, ptr: *const u8, len: u32);
    fn ff_log_data(ptr: *const u8, len: u32, log_level: u32);
}

// TODO: more to do here, hook up to 'impl log::Log for Logger'
pub struct Logger;

impl Logger {
    pub fn error(log: &str) {
        unsafe { ff_log_data(log.as_ptr(), log.len() as u32, LOG_LEVEL_ERROR) }
    }

    pub fn warn(log: &str) {
        unsafe { ff_log_data(log.as_ptr(), log.len() as u32, LOG_LEVEL_WARN) }
    }

    pub fn info(log: &str) {
        unsafe { ff_log_data(log.as_ptr(), log.len() as u32, LOG_LEVEL_INFO) }
    }

    pub fn debug(log: &str) {
        unsafe { ff_log_data(log.as_ptr(), log.len() as u32, LOG_LEVEL_DEBUG) }
    }

    pub fn trace(log: &str) {
        unsafe { ff_log_data(log.as_ptr(), log.len() as u32, LOG_LEVEL_TRACE) }
    }
}

pub trait Entity: Sized + PartialEq + Eq {
    const TYPE_ID: u64;

    fn from_row(vec: Vec<FtColumn>) -> Self;

    fn to_row(&self) -> Vec<FtColumn>;

    fn load(id: u64) -> Option<Self> {
        unsafe {
            let buf = id.to_le_bytes();
            let mut buflen = (buf.len() as u32).to_le_bytes();

            let ptr = ff_get_object(Self::TYPE_ID, buf.as_ptr(), buflen.as_mut_ptr());

            if !ptr.is_null() {
                let len = u32::from_le_bytes(buflen) as usize;
                let bytes = Vec::from_raw_parts(ptr, len, len);
                let vec = deserialize(&bytes);

                Some(Self::from_row(vec))
            } else {
                None
            }
        }
    }

    fn save(&self) {
        unsafe {
            let buf = serialize(&self.to_row());
            ff_put_object(Self::TYPE_ID, buf.as_ptr(), buf.len() as u32)
        }
    }
}

#[no_mangle]
fn alloc_fn(size: u32) -> *const u8 {
    let vec = Vec::with_capacity(size as usize);
    let ptr = vec.as_ptr();

    core::mem::forget(vec);

    ptr
}

#[no_mangle]
fn dealloc_fn(ptr: *mut u8, len: usize) {
    let _vec = unsafe { Vec::from_raw_parts(ptr, len, len) };
}
