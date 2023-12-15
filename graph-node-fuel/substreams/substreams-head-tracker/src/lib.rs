#![cfg(target_arch = "wasm32")]

#[no_mangle]
pub extern "C" fn map_blocks(_params_ptr: *mut u8, _params_len: usize) {}

#[no_mangle]
pub fn alloc(size: usize) -> *mut u8 {
    let mut buf = Vec::with_capacity(size);
    let ptr = buf.as_mut_ptr();

    // Runtime is responsible of calling dealloc when no longer needed
    std::mem::forget(buf);
    ptr
}

#[no_mangle]
pub unsafe fn dealloc(ptr: *mut u8, size: usize) {
    std::mem::drop(Vec::from_raw_parts(ptr, size, size))
}
