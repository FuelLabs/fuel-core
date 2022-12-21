use async_trait::async_trait;
use fuel_core_types::{
    blockchain::{
        block::Block,
        primitives::DaBlockHeight,
        SealedBlock,
        SealedBlockHeader,
    },
    fuel_tx::TxId,
};

#[async_trait]
pub trait Relayer {
    // wait until the relayer is synced with ethereum
    async fn await_synced() -> anyhow::Result<()>;
    // wait until a specific da_height is reached
    async fn await_da_height(da_height: DaBlockHeight) -> anyhow::Result<()>;
}

#[async_trait]
pub trait Executor {
    async fn validate_and_store_block(block: &Block) -> anyhow::Result<()>;
}

pub trait TransactionPool {
    // remove a set of txs from the pool after a block is committed
    fn drop_txs(txs: Vec<TxId>) -> anyhow::Result<()>;
}

pub trait Database {
    // insert consensus information associated with a sealed block
    fn insert_consensus_data(sealed_block: &SealedBlock) -> anyhow::Result<()>;
}

#[async_trait]
pub trait PeerToPeer {
    // broadcast the finalized header to peers who may need to sync
    async fn broadcast_sealed_header(
        sealed_block_header: SealedBlockHeader,
    ) -> anyhow::Result<()>;
}
