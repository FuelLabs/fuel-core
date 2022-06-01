use std::sync::Arc;

use fuel_tx::Transaction;
use tokio::sync::oneshot;

use super::model::{BlockHeight, ConsensusVote, FuelBlock, SealedFuelBlock};

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

pub enum P2PMpsc {
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
