//! Types related to block producer service.

use crate::{
    blockchain::header::{
        ConsensusParametersVersion,
        PartialBlockHeader,
    },
    fuel_tx::ContractId,
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

impl<T> Components<T> {
    /// Getter for the `ConsensusParametersVersion` of the future block.
    pub fn consensus_parameters_version(&self) -> ConsensusParametersVersion {
        self.header_to_produce
            .application
            .consensus_parameters_version
    }
}
