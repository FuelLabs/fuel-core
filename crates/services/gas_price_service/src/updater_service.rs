use crate::{
    common::fuel_core_storage_adapter::mint_values,
    sync_state::{
        SyncState,
        SyncStateNotifier,
        SyncStateObserver,
        new_sync_state_channel,
    },
    v1::service::LatestGasPrice,
};
use anyhow::anyhow;
use async_trait::async_trait;
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    StateWatcher,
    TaskNextAction,
    stream::BoxStream,
};
use fuel_core_types::services::block_importer::SharedImportResult;
use tokio_stream::StreamExt;

#[derive(Debug, Clone)]
pub struct SharedData {
    pub latest_gas_price: LatestGasPrice<u32, u64>,
    sync_observer: SyncStateObserver,
}

impl SharedData {
    pub async fn await_synced(&self) -> anyhow::Result<()> {
        let mut observer = self.sync_observer.clone();
        loop {
            if observer.borrow_and_update().is_synced() {
                break;
            }
            observer.changed().await?;
        }
        Ok(())
    }

    pub async fn await_synced_until(&self, block_height: &u32) -> anyhow::Result<()> {
        let mut observer = self.sync_observer.clone();
        loop {
            if observer.borrow_and_update().is_synced_until(block_height) {
                break;
            }
            observer.changed().await?;
        }
        Ok(())
    }
}

pub struct GasPriceUpdaterService {
    latest_gas_price: LatestGasPrice<u32, u64>,
    block_stream: BoxStream<SharedImportResult>,
    sync_notifier: SyncStateNotifier,
}

impl GasPriceUpdaterService {
    fn update_gas_price_from_block(
        &mut self,
        import_result: &SharedImportResult,
    ) -> anyhow::Result<()> {
        let block = &import_result.sealed_block.entity;
        let height: u32 = (*block.header().height()).into();

        let (_, gas_price) = mint_values(block).map_err(|e| {
            anyhow!("Failed to extract gas price from block {}: {:?}", height, e)
        })?;

        self.latest_gas_price.set(height, gas_price);
        tracing::debug!(
            "Updated latest gas price to {} at height {}",
            gas_price,
            height
        );

        self.sync_notifier.send_replace(SyncState::Synced(height));

        Ok(())
    }
}

pub struct UninitializedTask {
    latest_gas_price: LatestGasPrice<u32, u64>,
    block_stream: BoxStream<SharedImportResult>,
    sync_notifier: SyncStateNotifier,
}

impl UninitializedTask {
    pub fn new(
        latest_gas_price: LatestGasPrice<u32, u64>,
        block_stream: BoxStream<SharedImportResult>,
    ) -> Self {
        let (sync_notifier, _) = new_sync_state_channel();
        Self {
            latest_gas_price,
            block_stream,
            sync_notifier,
        }
    }
}

#[async_trait]
impl RunnableService for UninitializedTask {
    const NAME: &'static str = "GasPriceUpdaterService";
    type SharedData = SharedData;
    type Task = GasPriceUpdaterService;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        SharedData {
            latest_gas_price: self.latest_gas_price.clone(),
            sync_observer: self.sync_notifier.subscribe(),
        }
    }

    async fn into_task(
        self,
        _state_watcher: &StateWatcher,
        _params: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        Ok(GasPriceUpdaterService {
            latest_gas_price: self.latest_gas_price,
            block_stream: self.block_stream,
            sync_notifier: self.sync_notifier,
        })
    }
}

impl RunnableTask for GasPriceUpdaterService {
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        tokio::select! {
            biased;
            _ = watcher.while_started() => {
                TaskNextAction::Stop
            }
            maybe_import_result = self.block_stream.next() => {
                match maybe_import_result {
                    Some(import_result) => {
                        let res = self.update_gas_price_from_block(&import_result);
                        TaskNextAction::always_continue(res)
                    }
                    None => {
                        TaskNextAction::Stop
                    }
                }
            }
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}

pub fn new_gas_price_updater_service(
    latest_gas_price: LatestGasPrice<u32, u64>,
    block_stream: BoxStream<SharedImportResult>,
) -> fuel_core_services::ServiceRunner<UninitializedTask> {
    let task = UninitializedTask::new(latest_gas_price, block_stream);
    fuel_core_services::ServiceRunner::new(task)
}
