//! Defines the logic how to interact with the shared sequencer.

use crate::{
    http_api::AccountMetadata,
    ports::{
        BlocksProvider,
        Signer,
    },
    Client,
    Config,
};
use async_trait::async_trait;
use core::time::Duration;
use fuel_core_services::{
    stream::BoxStream,
    EmptyShared,
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
    TaskNextAction,
};
use fuel_core_types::services::{
    block_importer::SharedImportResult,
    shared_sequencer::{
        SSBlob,
        SSBlobs,
    },
};
use futures::StreamExt;
use std::sync::Arc;

/// Non-initialized shared sequencer task.
pub struct NonInitializedTask<S> {
    config: Config,
    signer: Arc<S>,
    blocks_events: BoxStream<SharedImportResult>,
}

/// Initialized shared sequencer task.
pub struct Task<S> {
    /// The client that communicates with shared sequencer.
    shared_sequencer_client: Option<Client>,
    config: Config,
    signer: Arc<S>,
    account_metadata: Option<AccountMetadata>,
    prev_order: Option<u64>,
    blobs: Arc<tokio::sync::Mutex<SSBlobs>>,
    interval: tokio::time::Interval,
}

impl<S> NonInitializedTask<S> {
    /// Create a new shared sequencer task.
    fn new(
        config: Config,
        blocks_events: BoxStream<SharedImportResult>,
        signer: Arc<S>,
    ) -> anyhow::Result<Self> {
        if config.enabled && config.endpoints.is_none() {
            return Err(anyhow::anyhow!(
                "Shared sequencer is enabled but no endpoints are set"
            ));
        }

        Ok(Self {
            config,
            blocks_events,
            signer,
        })
    }
}

#[async_trait]
impl<S> RunnableService for NonInitializedTask<S>
where
    S: Signer + 'static,
{
    const NAME: &'static str = "SharedSequencer";

    type SharedData = EmptyShared;
    type Task = Task<S>;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        EmptyShared
    }

    async fn into_task(
        mut self,
        _: &StateWatcher,
        _: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        let shared_sequencer_client = if let Some(endpoints) = &self.config.endpoints {
            let ss = Client::new(endpoints.clone(), self.config.topic).await?;

            if self.signer.is_available() {
                let cosmos_public_address = ss.sender_account_id(self.signer.as_ref())?;

                tracing::info!(
                    "Shared sequencer uses account ID: {}",
                    cosmos_public_address
                );
            }

            Some(ss)
        } else {
            None
        };

        let blobs = Arc::new(tokio::sync::Mutex::new(SSBlobs::new()));

        if self.config.enabled {
            let mut block_events = self.blocks_events;

            tokio::task::spawn({
                let blobs = blobs.clone();
                async move {
                    while let Some(block) = block_events.next().await {
                        let blob = SSBlob {
                            block_height: *block.sealed_block.entity.header().height(),
                            block_id: block.sealed_block.entity.id(),
                        };
                        blobs.lock().await.push(blob);
                    }
                }
            });
        }

        Ok(Task {
            interval: tokio::time::interval(self.config.block_posting_frequency),
            shared_sequencer_client,
            config: self.config,
            signer: self.signer,
            account_metadata: None,
            prev_order: None,
            blobs,
        })
    }
}

impl<S> Task<S>
where
    S: Signer,
{
    /// Fetch latest account metadata if it's not set
    async fn ensure_account_metadata(&mut self) -> anyhow::Result<()> {
        if self.account_metadata.is_some() {
            return Ok(());
        }
        let ss = self
            .shared_sequencer_client
            .as_ref()
            .expect("Shared sequencer client is not set");
        self.account_metadata = Some(ss.get_account_meta(self.signer.as_ref()).await?);
        Ok(())
    }

    /// Fetch previous order in the topic if it's not set
    async fn ensure_prev_order(&mut self) -> anyhow::Result<()> {
        if self.prev_order.is_some() {
            return Ok(());
        }
        let ss = self
            .shared_sequencer_client
            .as_ref()
            .expect("Shared sequencer client is not set");
        self.prev_order = ss.get_topic().await?.map(|f| f.order);
        Ok(())
    }
}

#[async_trait]
impl<S> RunnableTask for Task<S>
where
    S: Signer + 'static,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        if !self.config.enabled {
            let _ = watcher.while_started().await;
            return TaskNextAction::Stop;
        }

        if let Err(err) = self.ensure_account_metadata().await {
            // We don't want to spam the RPC endpoint with a lot of queries,
            // so wait for one second before sending the next one.
            tokio::time::sleep(Duration::from_secs(1)).await;
            return TaskNextAction::ErrorContinue(err)
        }
        if let Err(err) = self.ensure_prev_order().await {
            return TaskNextAction::ErrorContinue(err)
        };

        tokio::select! {
            biased;
            _ = watcher.while_started() => {
                TaskNextAction::Stop
            },
            _ = self.interval.tick() => {
                let blobs = {
                    let mut lock = self.blobs.lock().await;
                    core::mem::take(&mut *lock)
                };
                if blobs.is_empty() {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    return TaskNextAction::Continue;
                };

                let mut account = self.account_metadata.take().expect("Account metadata is not set");
                let next_order = self.prev_order.map(|prev| prev.wrapping_add(1)).unwrap_or(0);
                let ss =  self.shared_sequencer_client
                    .as_ref().expect("Shared sequencer client is not set");
                let blobs_bytes = postcard::to_allocvec(&blobs).expect("Failed to serialize SSBlob");

                if let Err(err) = ss.send(self.signer.as_ref(), account, next_order, blobs_bytes).await {
                    TaskNextAction::ErrorContinue(err);
                }

                tracing::info!("Posted block to shared sequencer {blobs:?}");
                account.sequence = account.sequence.saturating_add(1);
                self.prev_order = Some(next_order);
                self.account_metadata = Some(account);
                TaskNextAction::Continue
            },
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        // Nothing to shut down because we don't have any temporary state that should be dumped,
        // and we don't spawn any sub-tasks that we need to finish or await.
        Ok(())
    }
}

/// Creates an instance of runnable shared sequencer service.
pub fn new_service<B, S>(
    block_provider: B,
    config: Config,
    signer: Arc<S>,
) -> anyhow::Result<ServiceRunner<NonInitializedTask<S>>>
where
    B: BlocksProvider,
    S: Signer,
{
    let blocks_events = block_provider.subscribe();
    Ok(ServiceRunner::new(NonInitializedTask::new(
        config,
        blocks_events,
        signer,
    )?))
}
