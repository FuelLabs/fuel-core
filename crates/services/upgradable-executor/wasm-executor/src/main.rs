use crate::{
    relayer::WasmRelayer,
    storage::WasmStorage,
    tx_source::WasmTxSource,
    utils::{
        pack_ptr_and_len,
        ReturnType,
    },
};
use fuel_core_executor::executor::{
    default_consensus_parameters,
    ExecutionInstance,
};
use fuel_core_types::services::{
    block_producer::Components,
    executor::{
        Error as ExecutorError,
        ExecutionResult,
    },
    Uncommitted,
};

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
    pack_ptr_and_len(static_slice.as_ptr() as u32, static_slice.len() as u32)
}

pub fn execute_without_commit(component_len: u32, options_len: u32) -> ReturnType {
    let block = ext::input_component(component_len as usize)
        .map_err(|e| ExecutorError::Other(e.to_string()))?;
    let mut options = ext::input_options(options_len as usize)
        .map_err(|e| ExecutorError::Other(e.to_string()))?;

    let consensus_params = if let Some(consensus_params) = options.consensus_params.take()
    {
        consensus_params
    } else {
        default_consensus_parameters()
    };

    let instance = ExecutionInstance {
        relayer: WasmRelayer {},
        database: WasmStorage {},
        consensus_params,
        options,
    };

    let block = block.map_p(|component| {
        let Components {
            header_to_produce,
            gas_limit,
            coinbase_recipient,
            ..
        } = component;

        Components {
            header_to_produce,
            gas_limit,
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

fn main() {}
