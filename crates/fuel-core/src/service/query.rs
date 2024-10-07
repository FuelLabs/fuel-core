//! Queries we can run directly on `FuelService`.
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    fuel_tx::{
        Transaction,
        UniqueIdentifier,
    },
    fuel_types::Bytes32,
    services::txpool::TransactionStatus as TxPoolTxStatus,
};
use futures::{
    Stream,
    StreamExt,
};

use crate::{
    database::OffChainIterableKeyValueView,
    query::transaction_status_change,
    schema::tx::types::TransactionStatus,
};

use super::*;

impl FuelService {
    /// Submit a transaction to the txpool.
    pub async fn submit(&self, tx: Transaction) -> anyhow::Result<()> {
        self.shared
            .txpool_shared_state
            .insert(tx)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// Submit a transaction to the txpool and return a stream of status changes.
    pub async fn submit_and_status_change(
        &self,
        tx: Transaction,
    ) -> anyhow::Result<impl Stream<Item = anyhow::Result<TransactionStatus>>> {
        let id = tx.id(&self
            .shared
            .config
            .snapshot_reader
            .chain_config()
            .consensus_parameters
            .chain_id());
        let stream = self.transaction_status_change(id).await?;
        self.submit(tx).await?;
        Ok(stream)
    }

    /// Submit a transaction to the txpool and return the final status.
    pub async fn submit_and_await_commit(
        &self,
        tx: Transaction,
    ) -> anyhow::Result<TransactionStatus> {
        let id = tx.id(&self
            .shared
            .config
            .snapshot_reader
            .chain_config()
            .consensus_parameters
            .chain_id());
        let stream = self.transaction_status_change(id).await?.filter(|status| {
            futures::future::ready(!matches!(status, Ok(TransactionStatus::Submitted(_))))
        });
        futures::pin_mut!(stream);
        self.submit(tx).await?;
        stream
            .next()
            .await
            .ok_or_else(|| anyhow::anyhow!("Stream closed without transaction status"))?
    }

    /// Return a stream of status changes for a transaction.
    pub async fn transaction_status_change(
        &self,
        id: Bytes32,
    ) -> anyhow::Result<impl Stream<Item = anyhow::Result<TransactionStatus>>> {
        let txpool = self.shared.txpool_shared_state.clone();
        let db = self.shared.database.off_chain().latest_view()?;
        let rx = txpool.tx_update_subscribe(id)?;
        Ok(transaction_status_change(
            move |id| {
                Box::pin({
                    let txpool = txpool.clone();
                    let db = db.clone();
                    get_tx_status(db, txpool, id)
                }) as _
            },
            rx,
            id,
        )
        .await)
    }
}

async fn get_tx_status(
    db: OffChainIterableKeyValueView,
    txpool: TxPoolSharedState,
    id: Bytes32,
) -> StorageResult<Option<TxPoolTxStatus>> {
    match db.get_tx_status(&id)? {
        Some(status) => Ok(Some(status)),
        None => Ok(txpool
            .find_one(id)
            .await
            .map_err(|e| anyhow::anyhow!(e))?
            .map(Into::into)),
    }
}
