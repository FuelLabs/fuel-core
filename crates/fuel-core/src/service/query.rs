//! Queries we can run directly on `FuelService`.
use crate::graphql_api::ports::TxStatusManager;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    fuel_tx::{
        Transaction,
        UniqueIdentifier,
    },
    fuel_types::Bytes32,
    services::transaction_status::TransactionStatus as TxPoolTxStatus,
    tai64::Tai64,
};
use futures::{
    Stream,
    StreamExt,
};

use crate::{
    database::OffChainIterableKeyValueView,
    query::{
        TxnStatusChangeState,
        transaction_status_change,
    },
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
    ) -> anyhow::Result<impl Stream<Item = anyhow::Result<TransactionStatus>> + '_> {
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
            futures::future::ready(status.as_ref().is_ok_and(|status| status.is_final()))
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
    ) -> anyhow::Result<impl Stream<Item = anyhow::Result<TransactionStatus>> + '_> {
        // First subscribe to the statuses, and only after that create a view.
        let tx_status_manager = &self.shared.tx_status_manager;
        let rx = tx_status_manager.tx_update_subscribe(id).await?;
        let db = self.shared.database.off_chain().latest_view()?;
        let state = StatusChangeState {
            db,
            tx_status_manager,
        };
        Ok(transaction_status_change(state, rx, id, true).await)
    }
}

struct StatusChangeState<'a> {
    db: OffChainIterableKeyValueView,
    tx_status_manager: &'a TxStatusManagerAdapter,
}

impl TxnStatusChangeState for StatusChangeState<'_> {
    async fn get_tx_status(
        &self,
        id: Bytes32,
        include_preconfirmation: bool,
    ) -> StorageResult<Option<TxPoolTxStatus>> {
        match self.db.get_tx_status(&id)? {
            Some(status) => Ok(Some(status.into())),
            None => {
                let status = self.tx_status_manager.status(id).await?;
                let status = status.map(|status| {
                    // Filter out preconfirmation statuses if not allowed. Converting to submitted status
                    // because it's the closest to the preconfirmation status.
                    // Having `now()` as timestamp isn't ideal but shouldn't cause much inconsistency.
                    if !include_preconfirmation
                        && status.is_preconfirmation()
                        && !status.is_final()
                    {
                        TxPoolTxStatus::submitted(Tai64::now())
                    } else {
                        status
                    }
                });
                Ok(status)
            }
        }
    }
}
