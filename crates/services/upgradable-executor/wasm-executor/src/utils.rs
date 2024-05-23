use fuel_core_executor::executor::ExecutionOptions;
use fuel_core_storage::transactional::Changes;
use fuel_core_types::{
    blockchain::block::Block,
    services::{
        block_producer::Components,
        executor::{
            ExecutionResult,
            Result as ExecutorResult,
        },
        Uncommitted,
    },
};

/// Pack a pointer and length into an `u64`.
pub fn pack_ptr_and_len(ptr: u32, len: u32) -> u64 {
    (u64::from(len) << 32) | u64::from(ptr)
}

/// Unpacks an `u64` into the pointer and length.
pub fn unpack_ptr_and_len(val: u64) -> (u32, u32) {
    let ptr = u32::try_from(val & (u32::MAX as u64))
        .expect("It only contains first 32 bytes; qed");
    let len = u32::try_from(val >> 32).expect("It only contains first 32 bytes; qed");

    (ptr, len)
}

/// Pack a `exists`, `size` and `result` into one `u64`.
pub fn pack_exists_size_result(exists: bool, size: u32, result: u16) -> u64 {
    (u64::from(result) << 33 | u64::from(size) << 1) | u64::from(exists)
}

/// Unpacks an `u64` into `exists`, `size` and `result`.
pub fn unpack_exists_size_result(val: u64) -> (bool, u32, u16) {
    let exists = (val & 1u64) != 0;
    let size = u32::try_from((val >> 1) & (u32::MAX as u64))
        .expect("It only contains first 32 bytes; qed");
    let result = u16::try_from(val >> 33 & (u16::MAX as u64))
        .expect("It only contains first 16 bytes; qed");

    (exists, size, result)
}

/// The input type for the WASM executor. Enum allows handling different
/// versions of the input without introducing new host functions.
#[derive(Debug, serde::Serialize)]
pub enum InputSerializationType<'a> {
    V1 {
        block: WasmSerializationBlockTypes<'a, ()>,
        options: ExecutionOptions,
    },
}

#[derive(Debug, serde::Deserialize)]
pub enum InputDeserializationType {
    V1 {
        block: WasmDeserializationBlockTypes<()>,
        options: ExecutionOptions,
    },
}

#[derive(Debug, serde::Serialize)]
pub enum WasmSerializationBlockTypes<'a, TxSource> {
    /// DryRun mode where P is being produced.
    DryRun(Components<TxSource>),
    /// Production mode where P is being produced.
    Production(Components<TxSource>),
    /// Validation of a produced block.
    Validation(&'a Block),
}

#[derive(Debug, serde::Deserialize)]
pub enum WasmDeserializationBlockTypes<TxSource> {
    /// DryRun mode where P is being produced.
    DryRun(Components<TxSource>),
    /// Production mode where P is being produced.
    Production(Components<TxSource>),
    /// Validation of a produced block.
    Validation(Block),
}

/// The return type for the WASM executor. Enum allows handling different
/// versions of the return without introducing new host functions.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum ReturnType {
    V1(ExecutorResult<Uncommitted<ExecutionResult, Changes>>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::prop::*;

    proptest::proptest! {
        #[test]
        fn can_pack_any_values(exists: bool, size: u32, result: u16) {
            pack_exists_size_result(exists, size, result);
        }

        #[test]
        fn can_unpack_any_values(value: u64) {
            let _ = unpack_exists_size_result(value);
        }


        #[test]
        fn unpacks_packed_values(exists: bool, size: u32, result: u16) {
            let packed = pack_exists_size_result(exists, size, result);
            let (unpacked_exists, unpacked_size, unpacked_result) =
                unpack_exists_size_result(packed);

            proptest::prop_assert_eq!(exists, unpacked_exists);
            proptest::prop_assert_eq!(size, unpacked_size);
            proptest::prop_assert_eq!(result, unpacked_result);
        }
    }
}
