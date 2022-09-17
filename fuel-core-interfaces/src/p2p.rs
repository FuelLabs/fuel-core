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
use std::sync::Arc;
use tokio::sync::oneshot;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum TransactionBroadcast {
    NewTransaction(Transaction),
}

pub enum ConsensusBroadcast {
    NewVote(ConsensusVote),
}

pub enum BlockBroadcast {
    /// fuel block without consensus data
    NewBlock(FuelBlock),
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
    Stop,
}

#[async_trait]
pub trait P2pDb: Send + Sync {
    async fn get_sealed_block(&self, height: BlockHeight)
        -> Option<Arc<SealedFuelBlock>>;
}
