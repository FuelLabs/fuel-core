use thiserror::Error;
use wasmer::{
    Exports,
    ExportError,
    Function,
    HostEnvInitError,
    Instance,
    Memory,
    RuntimeError,
    Store,
    WasmPtr,
};
use fuel_indexer::types as ft;

use crate::runtime::IndexEnv;

#[derive(Debug, Error)]
pub enum FFIError {
    #[error("Invalid memory access")]
    MemoryBoundError,
    #[error("Error calling into wasm function {0:?}")]
    RuntimeError(#[from] RuntimeError),
    #[error("Error initializing host environment {0:?}")]
    HostEnvInitError(#[from] HostEnvInitError),
    #[error("Invalid export {0:?}")]
    ExportError(#[from] ExportError),
    #[error("Expected result from call {0:?}")]
    NoneError(String),
}


macro_rules! declare_export {
    ($name:ident, $ffi_env:ident, $store:ident, $env:ident) => {
        let f = Function::new_native_with_env($store, $env.clone(), $name);
        $ffi_env.insert(format!("ff_{}", stringify!($name)), f);
    }
}


pub(crate) fn get_namespace(instance: &Instance) -> Result<String, FFIError> {
    let exports = &instance.exports;
    let memory = exports.get_memory("memory")?;

    let ns_ptr = exports
        .get_function("get_namespace_ptr")?
        .call(&[])?[0].i32()
        .ok_or_else(
            || FFIError::NoneError("get_namespace".to_string())
        )? as u32;

    let ns_len = exports
        .get_function("get_namespace_len")?
        .call(&[])?[0].i32()
        .ok_or_else(
            || FFIError::NoneError("get_namespace".to_string())
        )? as u32;

    let namespace = get_string(&memory, ns_ptr, ns_len)?;

    Ok(namespace)
}


fn get_string(mem: &Memory, ptr: u32, len: u32) -> Result<String, FFIError> {
    let result = WasmPtr::<u8, wasmer::Array>::new(ptr)
        .get_utf8_string(mem, len)
        .ok_or_else(|| FFIError::MemoryBoundError)?;
    Ok(result)
}


fn get_object_id(mem: &Memory, ptr: u32) -> u64 {
    WasmPtr::<u64>::new(ptr).deref(mem).unwrap().get()
}


fn get_column_info(mem: &Memory, ptr: u32) -> (u32, u32) {
    let offset = core::mem::size_of::<u32>() as u32;
    (WasmPtr::<u32>::new(ptr).deref(mem).unwrap().get(),
    WasmPtr::<u32>::new(ptr + offset).deref(mem).unwrap().get())

}


fn put_object(env: &IndexEnv, type_id: u32, ptr: u32, len: u32) {
    let meta_size = core::mem::size_of::<u32>()*2;

    let mem = env.memory_ref().expect("Memory uninitialized");
    let mut columns = vec![];
    let mut offset = 0u32;

    unsafe {
        while offset < len {
            let (column_type, size) = get_column_info(&mem, ptr + offset);
            offset += meta_size as u32;

            let pos = ptr + offset;
            let range = pos as usize..pos as usize + size as usize;

            let col = ft::Column::new(
                column_type.into(),
                size as usize,
                &mem.data_unchecked()[range]
            );
            columns.push(col);
            offset += size;
        }
    }

    env.db.lock().expect("Acquire lock failed").put_object(type_id, columns);
}


fn get_object(env: &IndexEnv, type_id: u32, ptr: u32) -> u32 {
    let mem = env.memory_ref().expect("Memory uninitialized");

    unsafe {
        let id = get_object_id(&mem, ptr);
        let columns = env.db.lock()
            .expect("Lock acquire failed")
            .get_object(type_id, id);
        0
    }
}


pub fn get_exports(env: &IndexEnv, store: &Store) -> Exports {
    let mut exports = Exports::new();
    declare_export!(get_object, exports, store, env);
    declare_export!(put_object, exports, store, env);
    exports
}
