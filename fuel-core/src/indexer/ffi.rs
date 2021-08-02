use std::io::Read;
use wasmer::{
    Exports,
    Function,
    Store,
};
use fuel_tx::bytes::SizedBytes;

use crate::indexer::IndexEnv;

macro_rules! declare_export {
    ($name:ident, $ffi_env:ident, $store:ident, $env:ident) => {
        let f = Function::new_native_with_env($store, $env.clone(), $name);
        $ffi_env.insert(format!("ff_{}", stringify!($name)), f);
    }
}


fn get_transaction(env: &IndexEnv, ptr: u32, len: u32) -> u32 {
    let mem = env.memory_ref().expect("Memory uninitialized");
    let mut trans = env.db.get_transaction();
    let size = trans.serialized_size();

    unsafe {
        assert!(size < len as usize);
        let range = ptr as usize..ptr as usize + size;
        trans.read(&mut mem.data_unchecked_mut()[range]).expect("Could not serialize") as u32
    }
}


pub fn get_ffi_exports(env: &IndexEnv, store: &Store) -> Exports {
    let mut exports = Exports::new();
    declare_export!(get_transaction, exports, store, env);
    exports
}
