use super::ProofOfIndexingEvent;
use crate::prelude::DeploymentHash;
use crate::util::stable_hash_glue::{impl_stable_hash, AsBytes};
use std::collections::HashMap;
use web3::types::{Address, H256};

/// The PoI is the StableHash of this struct. This reference implementation is
/// mostly here just to make sure that the online implementation is
/// well-implemented (without conflicting sequence numbers, or other oddities).
/// It's just way easier to check that this works, and serves as a kind of
/// documentation as a side-benefit.
pub struct PoI<'a> {
    pub causality_regions: HashMap<String, PoICausalityRegion<'a>>,
    pub subgraph_id: DeploymentHash,
    pub block_hash: H256,
    pub indexer: Option<Address>,
}

fn h256_as_bytes(val: &H256) -> AsBytes<&[u8]> {
    AsBytes(val.as_bytes())
}

fn indexer_opt_as_bytes(val: &Option<Address>) -> Option<AsBytes<&[u8]>> {
    val.as_ref().map(|v| AsBytes(v.as_bytes()))
}

impl_stable_hash!(PoI<'_> {
    causality_regions,
    subgraph_id,
    block_hash: h256_as_bytes,
    indexer: indexer_opt_as_bytes
});

pub struct PoICausalityRegion<'a> {
    pub blocks: Vec<Block<'a>>,
}

impl_stable_hash!(PoICausalityRegion<'_> {blocks});

impl PoICausalityRegion<'_> {
    pub fn from_network(network: &str) -> String {
        format!("ethereum/{}", network)
    }
}

#[derive(Default)]
pub struct Block<'a> {
    pub events: Vec<ProofOfIndexingEvent<'a>>,
}

impl_stable_hash!(Block<'_> {events});
