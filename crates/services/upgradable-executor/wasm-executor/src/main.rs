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

use crate as fuel_core_wasm_executor;
use crate::utils::{
    InputDeserializationType,
    WasmDeserializationBlockTypes,
};
use fuel_core_executor::executor::ExecutionInstance;
use fuel_core_storage::transactional::Changes;
use fuel_core_types::{
    blockchain::block::Block,
    services::{
        block_producer::Components,
        executor::{
            Error as ExecutorError,
            ExecutionResult,
            Result as ExecutorResult,
        },
        Uncommitted,
    },
};
use fuel_core_wasm_executor::{
    relayer::WasmRelayer,
    storage::WasmStorage,
    tx_source::WasmTxSource,
    utils::{
        pack_ptr_and_len,
        ReturnType,
    },
};

mod ext;
mod relayer;
mod storage;
mod tx_source;
pub mod utils;

#[no_mangle]
pub extern "C" fn execute(input_len: u32) -> u64 {
    let result = execute_without_commit(input_len);
    let output = ReturnType::V1(result);
    let encoded = postcard::to_allocvec(&output).expect("Failed to encode the output");
    let static_slice = encoded.leak();
    pack_ptr_and_len(
        static_slice.as_ptr() as u32,
        u32::try_from(static_slice.len()).expect("We only support wasm32 target; qed"),
    )
}

pub fn execute_without_commit(
    input_len: u32,
) -> ExecutorResult<Uncommitted<ExecutionResult, Changes>> {
    let input = ext::input(input_len as usize)
        .map_err(|e| ExecutorError::Other(e.to_string()))?;

    let (block, options) = match input {
        InputDeserializationType::V1 { block, options } => {
            let block = match block {
                WasmDeserializationBlockTypes::DryRun(c) => {
                    WasmDeserializationBlockTypes::DryRun(use_wasm_tx_source(c))
                }
                WasmDeserializationBlockTypes::Production(c) => {
                    WasmDeserializationBlockTypes::Production(use_wasm_tx_source(c))
                }
                WasmDeserializationBlockTypes::Validation(c) => {
                    WasmDeserializationBlockTypes::Validation(c)
                }
            };

            (block, options)
        }
    };

    let instance = ExecutionInstance {
        relayer: WasmRelayer {},
        database: WasmStorage {},
        options,
    };

    match block {
        WasmDeserializationBlockTypes::DryRun(c) => execute_dry_run(instance, c),
        WasmDeserializationBlockTypes::Production(c) => execute_production(instance, c),
        WasmDeserializationBlockTypes::Validation(c) => execute_validation(instance, c),
    }
}

fn execute_dry_run(
    instance: ExecutionInstance<WasmRelayer, WasmStorage>,
    block: Components<WasmTxSource>,
) -> ExecutorResult<Uncommitted<ExecutionResult, Changes>> {
    instance.produce_without_commit(block, true)
}

fn execute_production(
    instance: ExecutionInstance<WasmRelayer, WasmStorage>,
    block: Components<WasmTxSource>,
) -> ExecutorResult<Uncommitted<ExecutionResult, Changes>> {
    instance.produce_without_commit(block, false)
}

fn execute_validation(
    instance: ExecutionInstance<WasmRelayer, WasmStorage>,
    block: Block,
) -> ExecutorResult<Uncommitted<ExecutionResult, Changes>> {
    let result = instance
        .validate_without_commit(&block)?
        .into_execution_result(block, vec![]);

    // TODO: Modify return type to differentiate between validation and production results
    // https://github.com/FuelLabs/fuel-core/issues/1887
    Ok(result)
}

fn use_wasm_tx_source(component: Components<()>) -> Components<WasmTxSource> {
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
}

// It is not used. It was added to make clippy happy.
fn main() {}
