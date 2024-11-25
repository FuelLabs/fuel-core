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
    // This function is not cancel-safe because it calls `sleep` inside.
    async fn blobs(&mut self) -> anyhow::Result<Option<SSBlobs>> {
        let ss = self
            .shared_sequencer_client
            .as_ref()
            .expect("Shared sequencer client is not set; qed");

        if self.account_metadata.is_none() {
            // If the account is not funded, this code will fail
            // because we can't sign the transaction without account metadata.
            let account_metadata = ss.get_account_meta(self.signer.as_ref()).await;

            match account_metadata {
                Ok(account_metadata) => {
                    self.account_metadata = Some(account_metadata);
                }
                Err(err) => {
                    // We don't want to spam the RPC endpoint with a lot of queries,
                    // so wait for one second before sending the next on.
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    return Err(err);
                }
            }
        }

        if self.prev_order.is_none() {
            self.prev_order = ss.get_topic().await?.map(|f| f.order);
        }

        let blobs = {
            let mut lock = self.blobs.lock().await;
            core::mem::take(&mut *lock)
        };

        if blobs.is_empty() {
            tokio::time::sleep(self.config.block_posting_frequency).await;
            Ok(None)
        } else {
            Ok(Some(blobs))
        }
    }
}

const CONTINUE: bool = true;
const STOP: bool = false;

#[async_trait]
impl<S> RunnableTask for Task<S>
where
    S: Signer + 'static,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        if !self.config.enabled {
            let _ = watcher.while_started().await;
            return Ok(STOP);
        }

        tokio::select! {
            biased;
            _ = watcher.while_started() => {
                Ok(STOP)
            },

            // The `blobs` function is not cancel safe, as it calls sleep inside.
            // If someone add new logic into the `tokio::select`, please
            // rework the `blobs` function to be cancel safe and use interval inside.
            blobs = self.blobs() => {
                let blobs = blobs?;

                if let Some(blobs) = blobs {
                    let mut account = self.account_metadata.take().expect("Account metadata is not set; qed");
                    let next_order = if let Some(prev_order) = self.prev_order {
                        prev_order.wrapping_add(1)
                    } else {
                        0
                    };

                    let ss =  self.shared_sequencer_client
                        .as_ref().expect("Shared sequencer client is not set; qed");
                    let blobs_bytes = postcard::to_allocvec(&blobs).expect("Failed to serialize SSBlob");
                    let result = ss.send(self.signer.as_ref(), account, next_order, blobs_bytes).await;

                    match result {
                        Ok(_) => {
                            tracing::info!("Posted block to shared sequencer {blobs:?}");
                            account.sequence = account.sequence.saturating_add(1);
                            self.prev_order = Some(next_order);
                            self.account_metadata = Some(account);
                            Ok(CONTINUE)
                        }
                        Err(err) => {
                            Err(err)
                        }
                    }
                } else {
                    Ok(CONTINUE)
                }
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
