//! Types related to block producer service.

use crate::{
    blockchain::{
        block::Block,
        header::{
            PartialBlockHeader,
            StateTransitionBytecodeVersion,
        },
    },
    fuel_tx::ContractId,
    services::executor::ExecutionTypes,
};

/// The components required to produce a block.
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Components<Source> {
    /// The partial block header of the future block without transactions related information.
    pub header_to_produce: PartialBlockHeader,
    /// The source of transactions potentially includable into the future block.
    /// It can be a predefined vector of transactions, a stream of transactions,
    /// or any other type that carries the transactions.
    pub transactions_source: Source,
    /// The `ContractId` of the fee recipient.
    pub coinbase_recipient: ContractId,
    /// The gas price for all transactions in the block.
    pub gas_price: u64,
}

impl<TxSource> ExecutionTypes<Components<TxSource>, Block> {
    /// Returns the state transition bytecode version of the block.
    pub fn state_transition_version(&self) -> StateTransitionBytecodeVersion {
        match self {
            ExecutionTypes::DryRun(component) => {
                component
                    .header_to_produce
                    .state_transition_bytecode_version
            }
            ExecutionTypes::Production(component) => {
                component
                    .header_to_produce
                    .state_transition_bytecode_version
            }
            ExecutionTypes::Validation(block) => {
                block.header().state_transition_bytecode_version
            }
        }
    }
}
