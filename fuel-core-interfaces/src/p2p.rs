use super::model::{BlockHeight, ConsensusVote, FuelBlock, SealedFuelBlock};
use fuel_tx::Transaction;
use std::sync::Arc;
use tokio::sync::oneshot;

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

pub enum P2pMpsc {
    RequestBlock {
        height: BlockHeight,
        response: oneshot::Sender<SealedFuelBlock>,
    },
    BroadcastNewTransaction {
        tx: Arc<Transaction>,
    },
    BroadcastNewBlock {
        block: Arc<FuelBlock>,
    },
}
