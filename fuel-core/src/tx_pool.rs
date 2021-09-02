use crate::executor::Executor;
use crate::model::fuel_block::FuelBlock;
use crate::model::Hash;
use async_graphql::futures_util::FutureExt;
use fuel_vm::prelude::Transaction;
use futures::prelude::stream::{self, BoxStream};
use futures::StreamExt;
use itertools::Itertools;
use std::convert::TryInto;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use thiserror::Error;

pub enum TransactionStatus {
    Submitted,
    InBlock { block_id: Hash },
    Failed { reason: String },
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("unable to process transaction")]
    InvalidTransaction { reason: String },
}

/// Holds submitted transactions and attempts to propose blocks
struct TxPool {
    executor: Executor,
    block: AtomicU64,
}

impl TxPool {
    pub fn new(executor: Executor) -> Self {
        TxPool {
            executor,
            block: AtomicU64::new(0),
        }
    }

    pub async fn submit_tx(
        &self,
        tx: Transaction,
    ) -> Result<BoxStream<'_, TransactionStatus>, Error> {
        let mut tx_updates: Vec<TransactionStatus> = vec![];
        tx_updates.push(TransactionStatus::Submitted);

        // setup and execute block
        let mut block_id = [0u8; 24].to_vec();
        block_id.extend_from_slice(&self.block.load(Ordering::SeqCst).to_be_bytes());
        let block_id: Hash = block_id
            .try_into()
            .expect("block id is constructed to the exact length");

        let evt = self
            .executor
            .execute(FuelBlock {
                id: block_id.clone(),
                fuel_height: self.block.fetch_add(1, Ordering::SeqCst),
                transactions: vec![],
            })
            .await
            .map_or_else(
                |e| TransactionStatus::Failed {
                    reason: format!("{:?}", e),
                },
                |_| TransactionStatus::InBlock { block_id },
            );
        tx_updates.push(evt);

        // convert into stream
        Ok(stream::iter(tx_updates).boxed())
    }
}
