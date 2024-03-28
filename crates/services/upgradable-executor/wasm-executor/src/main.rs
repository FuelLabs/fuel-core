//! This is the main entry point for the wasm executor.
//! The module defines the `execute` function that the host will call.
//! The result of the execution is the `ExecutionResult` with the list of changes to the storage.
//!
//! During return, the result of the execution modules leaks the memory,
//! allowing the WASM runner to get access to the data.
//!
//! Currently, the WASM executor is designed only for one block execution per WASM instance.
//! But later, it will be improved, and the instance will be reusable.

#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

use crate::{
    relayer::WasmRelayer,
    storage::WasmStorage,
    tx_source::WasmTxSource,
    utils::{
        pack_ptr_and_len,
        ReturnType,
    },
};
use fuel_core_executor::executor::ExecutionInstance;
use fuel_core_types::services::{
    block_producer::Components,
    executor::{
        Error as ExecutorError,
        ExecutionResult,
    },
    Uncommitted,
};
use fuel_core_wasm_executor as _;

mod ext;
mod relayer;
mod storage;
mod tx_source;
pub mod utils;

#[no_mangle]
pub extern "C" fn execute(component_len: u32, options_len: u32) -> u64 {
    let result = execute_without_commit(component_len, options_len);
    let encoded = postcard::to_allocvec(&result).expect("Failed to encode the result");
    let static_slice = encoded.leak();
    pack_ptr_and_len(
        static_slice.as_ptr() as u32,
        u32::try_from(static_slice.len()).expect("We only support wasm32 target; qed"),
    )
}

pub fn execute_without_commit(component_len: u32, options_len: u32) -> ReturnType {
    let block = ext::input_component(component_len as usize)
        .map_err(|e| ExecutorError::Other(e.to_string()))?;
    let options = ext::input_options(options_len as usize)
        .map_err(|e| ExecutorError::Other(e.to_string()))?;

    let instance = ExecutionInstance {
        relayer: WasmRelayer {},
        database: WasmStorage {},
        options,
    };

    let block = block.map_p(|component| {
        let Components {
            header_to_produce,
            gas_price,
            coinbase_recipient,
            ..
        } = component;

        Components {
            header_to_produce,
            gas_price,
            transactions_source: WasmTxSource::new(),
            coinbase_recipient,
        }
    });

    let (
        ExecutionResult {
            block,
            skipped_transactions,
            tx_status,
            events,
        },
        changes,
    ) = instance.execute_without_commit(block)?.into();

    Ok(Uncommitted::new(
        ExecutionResult {
            block,
            skipped_transactions,
            tx_status,
            events,
        },
        changes,
    ))
}

// It is not used. It was added to make clippy happy.
fn main() {}
