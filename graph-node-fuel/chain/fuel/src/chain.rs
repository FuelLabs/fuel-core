use std::sync::Arc;
use graph::blockchain::{BlockchainKind, RuntimeAdapter};
use graph::firehose::FirehoseEndpoint;
use graph::prelude::async_trait;
use crate::adapter::TriggerFilter;
use crate::capabilities::NodeCapabilities;
use crate::codec;
use crate::data_source::DataSource;

pub struct Chain { /*...*/ }

#[async_trait]
impl Blockchain for Chain {
    const KIND: BlockchainKind = BlockchainKind::Fuel;

    type Block = codec::Block;

    type DataSource = DataSource;

    // ...

    type TriggerFilter = TriggerFilter;

    type NodeCapabilities = NodeCapabilities;

    type RuntimeAdapter = RuntimeAdapter;
}

pub struct TriggersAdapter { /*...*/ }

#[async_trait]
impl TriggersAdapterTrait<Chain> for TriggersAdapter { /*...*/ }

pub struct FirehoseMapper {
    endpoint: Arc<FirehoseEndpoint>,
}

#[async_trait]
impl FirehoseMapperTrait<Chain> for FirehoseMapper { /*...*/ }