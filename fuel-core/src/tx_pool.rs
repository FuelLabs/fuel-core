use fuel_core::model::Hash;
use fuel_vm::prelude::Transaction;
use futures::prelude::stream::{self, BoxStream};
use std::thread::spawn;
use thiserror::Error;

pub enum TransactionStatus {
    Submitted,
    InBlock { block_id: Hash },
}

#[derive(Error)]
pub enum Error {}

/// Holds submitted transactions and attempts to propose blocks
struct TxPool {}

impl TxPool {
    pub async fn submit_tx(
        tx: Transaction,
    ) -> Result<BoxStream<'static, TransactionStatus>, Error> {
        let stream = stream::
        let status_stream = spawn(async {});

        Ok(())
    }
}
