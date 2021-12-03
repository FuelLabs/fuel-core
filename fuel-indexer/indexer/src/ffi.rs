use fuel_indexer_schema::FtColumn;
use thiserror::Error;
use wasmer::{
    ExportError, Exports, Function, HostEnvInitError, Instance, Memory, RuntimeError, Store,
    WasmPtr,
};

use crate::IndexEnv;

#[derive(Debug, Error)]
pub enum FFIError {
    #[error("Invalid memory access")]
    MemoryBound,
    #[error("Error calling into wasm function {0:?}")]
    Runtime(#[from] RuntimeError),
    #[error("Error initializing host environment {0:?}")]
    HostEnvInit(#[from] HostEnvInitError),
    #[error("Invalid export {0:?}")]
    Export(#[from] ExportError),
    #[error("Expected result from call {0:?}")]
    None(String),
}

macro_rules! declare_export {
    ($name:ident, $ffi_env:ident, $store:ident, $env:ident) => {
        let f = Function::new_native_with_env($store, $env.clone(), $name);
        $ffi_env.insert(format!("ff_{}", stringify!($name)), f);
    };
}

pub(crate) fn get_namespace(instance: &Instance) -> Result<String, FFIError> {
    let exports = &instance.exports;
    let memory = exports.get_memory("memory")?;

    let ptr = exports.get_function("get_namespace_ptr")?.call(&[])?[0]
        .i32()
        .ok_or_else(|| FFIError::None("get_namespace".to_string()))? as u32;

    let len = exports.get_function("get_namespace_len")?.call(&[])?[0]
        .i32()
        .ok_or_else(|| FFIError::None("get_namespace".to_string()))? as u32;

    let namespace = get_string(memory, ptr, len)?;

    Ok(namespace)
}

pub(crate) fn get_version(instance: &Instance) -> Result<String, FFIError> {
    let exports = &instance.exports;
    let memory = exports.get_memory("memory")?;

    let ptr = exports.get_function("get_version_ptr")?.call(&[])?[0]
        .i32()
        .ok_or_else(|| FFIError::None("get_version".to_string()))? as u32;

    let len = exports.get_function("get_version_len")?.call(&[])?[0]
        .i32()
        .ok_or_else(|| FFIError::None("get_version".to_string()))? as u32;

    let namespace = get_string(memory, ptr, len)?;

    Ok(namespace)
}

fn get_string(mem: &Memory, ptr: u32, len: u32) -> Result<String, FFIError> {
    let result = WasmPtr::<u8, wasmer::Array>::new(ptr)
        .get_utf8_string(mem, len)
        .ok_or(FFIError::MemoryBound)?;
    Ok(result)
}

fn get_object_id(mem: &Memory, ptr: u32) -> u64 {
    WasmPtr::<u64>::new(ptr).deref(mem).unwrap().get()
}

fn get_object(env: &IndexEnv, type_id: u64, ptr: u32, len_ptr: u32) -> u32 {
    let mem = env.memory_ref().expect("Memory uninitialized");

    let id = get_object_id(mem, ptr);

    let bytes = env
        .db
        .lock()
        .expect("Lock acquire failed")
        .get_object(type_id, id);

    if let Some(bytes) = bytes {
        let alloc_fn = env.alloc_ref().expect("Alloc export is missing");

        let size = bytes.len() as u32;
        let result = alloc_fn.call(size).expect("Alloc failed");
        let range = result as usize..result as usize + size as usize;

        WasmPtr::<u32>::new(len_ptr).deref(mem).unwrap().set(size);

        unsafe {
            mem.data_unchecked_mut()[range].copy_from_slice(&bytes);
        }

        result
    } else {
        0
    }
}

fn put_object(env: &IndexEnv, type_id: u64, ptr: u32, len: u32) {
    let mem = env.memory_ref().expect("Memory uninitialized");

    let mut bytes = Vec::with_capacity(len as usize);
    let range = ptr as usize..ptr as usize + len as usize;

    unsafe {
        bytes.extend_from_slice(&mem.data_unchecked()[range]);
    }

    let columns: Vec<FtColumn> = serde_scale::from_slice(&bytes).expect("Scale serde error");

    env.db
        .lock()
        .expect("Acquire lock failed")
        .put_object(type_id, columns, bytes);
}

pub fn get_exports(env: &IndexEnv, store: &Store) -> Exports {
    let mut exports = Exports::new();
    declare_export!(get_object, exports, store, env);
    declare_export!(put_object, exports, store, env);
    exports
}
