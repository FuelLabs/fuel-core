//! Types related to block producer service.

use crate::blockchain::header::PartialBlockHeader;

/// The components required to produce a block.
#[derive(Debug)]
pub struct Components<Source> {
    /// The partial block header of the future block without transactions related information.
    pub header_to_produce: PartialBlockHeader,
    /// The source of transactions potentially includable into the future block.
    /// It can be a predefined vector of transactions, a stream of transactions,
    /// or any other type that carries the transactions.
    pub transactions_source: Source,
    /// The gas limit of the block.
    pub gas_limit: u64,
}
