use super::model::{
    BlockHeight,
    FuelBlock,
    SealedFuelBlock,
};
use crate::{
    common::fuel_tx::Transaction,
    model::ConsensusVote,
};
use async_trait::async_trait;
use serde::{
    Deserialize,
    Serialize,
};
use std::{
    fmt::Debug,
    sync::Arc,
};
use tokio::sync::oneshot;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum TransactionBroadcast {
    NewTransaction(Transaction),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ConsensusBroadcast {
    NewVote(ConsensusVote),
}

#[derive(Debug, Clone)]
pub enum BlockBroadcast {
    /// fuel block without consensus data
    NewBlock(FuelBlock),
}

#[derive(Debug)]
pub enum GossipsubMessageAcceptance {
    Accept,
    Reject,
    Ignore,
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, PartialEq, Eq)]
pub struct GossipsubMessageId(pub Vec<u8>);

impl From<Vec<u8>> for GossipsubMessageId {
    fn from(message_id: Vec<u8>) -> Self {
        GossipsubMessageId(message_id)
    }
}

#[derive(Debug)]
pub enum P2pRequestEvent {
    RequestBlock {
        height: BlockHeight,
        response: oneshot::Sender<SealedFuelBlock>,
    },
    BroadcastNewTransaction {
        transaction: Arc<Transaction>,
    },
    BroadcastNewBlock {
        block: Arc<FuelBlock>,
    },
    BroadcastConsensusVote {
        vote: Arc<ConsensusVote>,
    },
    GossipsubMessageReport {
        message: Box<dyn NetworkData>,
        acceptance: GossipsubMessageAcceptance,
    },
    Stop,
}

pub trait NetworkData: Debug + Send {
    fn take_data(&mut self) -> Option<Box<dyn Debug>>;
    fn message_id(&self) -> Vec<u8>;
    fn peer_id(&self) -> Vec<u8>;
}

#[async_trait]
pub trait P2pDb: Send + Sync {
    async fn get_sealed_block(&self, height: BlockHeight)
        -> Option<Arc<SealedFuelBlock>>;
}
