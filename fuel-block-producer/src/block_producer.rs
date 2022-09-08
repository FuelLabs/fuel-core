use crate::{
    block_producer::transaction_selector::select_transactions,
    db::BlockProducerDatabase,
    ports::{
        Relayer,
        TxPool,
    },
    Config,
};
use anyhow::Result;
use chrono::Utc;
use fuel_core_interfaces::{
    block_producer::BlockProducerMpsc,
    common::{
        fuel_tx::CheckedTransaction,
        fuel_types::Bytes32,
    },
    executor::{
        ExecutionMode,
        Executor,
    },
    model::{
        BlockHeight,
        DaBlockHeight,
        FuelBlock,
        FuelBlockHeader,
    },
};
use std::{
    cmp::max,
    sync::Arc,
};
use tokio::sync::mpsc;
use tracing::{
    info,
    warn,
};

mod transaction_selector;

/// The default distance to trail the DA layer. We trail the finalized da height by some
/// margin to ensure all peers have adequate time to finalize the same blocks.
pub const DA_HEIGHT_TRAIL: u64 = 10;

pub struct Task {
    pub receiver: mpsc::Receiver<BlockProducerMpsc>,
    pub config: Config,
    pub db: Box<dyn BlockProducerDatabase>,
    pub relayer: Box<dyn Relayer>,
    pub txpool: Box<dyn TxPool>,
    pub executor: Box<dyn Executor>,
}

impl Task {
    pub async fn spawn(mut self) -> mpsc::Receiver<BlockProducerMpsc> {
        loop {
            tokio::select! {
                maybe_event = self.receiver.recv() => {
                    if !self.event_handler(maybe_event).await {
                        // stop task if event handler indicates stop
                        break;
                    }
                }
            }
        }
        info!("block producer service stopped.");
        self.receiver
    }

    async fn event_handler(&mut self, event: Option<BlockProducerMpsc>) -> bool {
        match event {
            Some(BlockProducerMpsc::Produce { height, response }) => {
                info!(
                    "handling block production request for height {}, with validator id {}",
                    height, self.config.validator_id
                );
                let produced_block =
                    self.produce_block(height).await.map(Box::new).map(Arc::new);
                if let Err(e) = response.send(produced_block) {
                    // this isn't strictly an error since a block production request can be started
                    // at any time from anywhere.
                    warn!("block production requester dropped response: {:?}", e)
                }

                // allow task to continue processing
                true
            }
            Some(BlockProducerMpsc::Stop) => false,
            None => false,
        }
    }

    async fn produce_block(&self, height: BlockHeight) -> Result<FuelBlock> {
        //  - get previous block info (hash, root, etc)
        //  - select reasonable da_height from relayer
        //  - get current consensus key from relayer based on selected da_height
        //  - get available txs from txpool
        //  - select best txs based on factors like:
        //      1. fees
        //      2. parallel throughput
        //  - Execute block with production mode to correctly malleate txs outputs and block headers
        //  - Sign block with production key

        let previous_block_info = self.previous_block_info(height).await?;
        let new_da_height = self
            .select_new_da_height(previous_block_info.da_height)
            .await?;
        let producer_id = self
            .relayer
            .get_block_production_key(self.config.validator_id, new_da_height)
            .await?;
        let best_transactions = self.select_best_transactions(height).await?;

        let header = FuelBlockHeader {
            height,
            number: new_da_height,
            parent_hash: previous_block_info.hash,
            prev_root: previous_block_info.transaction_root,
            transactions_root: Default::default(),
            time: Utc::now(),
            producer: producer_id,
            metadata: None,
        };
        let mut block = FuelBlock {
            header,
            transactions: best_transactions.into_iter().map(Into::into).collect(),
        };
        self.executor
            .execute(&mut block, ExecutionMode::Production)
            .await?;

        Ok(block)
    }

    async fn select_new_da_height(
        &self,
        previous_da_height: DaBlockHeight,
    ) -> Result<DaBlockHeight> {
        let trailed_best_height = self
            .relayer
            .get_best_finalized_da_height()
            .await?
            .saturating_sub(DA_HEIGHT_TRAIL);
        // we prefer to use the trailed_best_height, but use max to ensure height is
        // at least as good as previous block.
        Ok(max(trailed_best_height, previous_da_height))
    }

    async fn select_best_transactions(
        &self,
        block_height: BlockHeight,
    ) -> Result<Vec<CheckedTransaction>> {
        let includable_txs = self.txpool.get_includable_txs().await?;
        // TODO: The transaction pool should return transactions that are already checked
        let includable_txs = includable_txs
            .into_iter()
            .map(|tx| {
                CheckedTransaction::check(
                    (*tx).clone(),
                    block_height.into(),
                    &self.config.consensus_params,
                )
            })
            .collect::<Result<_, _>>()?;
        Ok(select_transactions(includable_txs, &self.config))
    }

    async fn previous_block_info(
        &self,
        height: BlockHeight,
    ) -> Result<PreviousBlockInfo> {
        // if this is the first block (ooh wee!), fill in base metadata from genesis
        if height <= 1u32.into() {
            // use best finalized height for first block
            let best_da_height = self.relayer.get_best_finalized_da_height().await?;
            Ok(PreviousBlockInfo {
                hash: Default::default(),
                transaction_root: Default::default(),
                da_height: best_da_height,
            })
        } else {
            // get info from previous block height
            let previous_block = self.db.get_block(height - 1u32.into())?;
            Ok(PreviousBlockInfo {
                hash: previous_block.id(),
                transaction_root: previous_block.header.transactions_root,
                da_height: previous_block.header.number,
            })
        }
    }
}

struct PreviousBlockInfo {
    hash: Bytes32,
    transaction_root: Bytes32,
    da_height: DaBlockHeight,
}
