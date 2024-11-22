//! This module handles bridge communications between the fuel node and the data availability layer.

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
use fuel_core_types::services::block_importer::SharedImportResult;
use futures::StreamExt;
use std::sync::Arc;

/// Non-initialized shared sequencer task.
pub struct NonInitializedTask<S> {
    /// The client that communicates with shared sequencer.
    shared_sequencer_client: Client,
    signer: Arc<S>,
    blocks_events: BoxStream<SharedImportResult>,
}

/// Initialized shared sequencer task.
pub struct Task<S> {
    /// The client that communicates with shared sequencer.
    shared_sequencer_client: Client,
    signer: Arc<S>,
    account_metadata: Option<AccountMetadata>,
    prev_order: u64,
    blocks_events: BoxStream<SharedImportResult>,
}

impl<S> NonInitializedTask<S> {
    /// Create a new shared sequencer task.
    fn new(
        config: Config,
        blocks_events: BoxStream<SharedImportResult>,
        signer: Arc<S>,
    ) -> Self {
        let shared_sequencer_client = Client::new(config);
        Self {
            shared_sequencer_client,
            blocks_events,
            signer,
        }
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
        // If the account is not funded, this code will fail
        // because we can't sign the transaction without account metadata.
        let account_metadata = self
            .shared_sequencer_client
            .get_account_meta(self.signer.as_ref())
            .await?;
        let account_metadata = Some(account_metadata);

        let prev_order = self
            .shared_sequencer_client
            .get_topic()
            .await?
            .map(|f| f.order)
            .unwrap_or_default();

        Ok(Task {
            shared_sequencer_client: self.shared_sequencer_client,
            signer: self.signer,
            account_metadata,
            prev_order,
            blocks_events: self.blocks_events,
        })
    }
}

#[async_trait]
impl<S> RunnableTask for Task<S>
where
    S: Signer + 'static,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        if self.account_metadata.is_none() {
            let account_metadata = self
                .shared_sequencer_client
                .get_account_meta(self.signer.as_ref())
                .await;

            match account_metadata {
                Ok(account_metadata) => {
                    self.account_metadata = Some(account_metadata);
                }
                Err(err) => {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    return TaskNextAction::ErrorContinue(err);
                }
            }
        }

        tokio::select! {
            biased;
            _ = watcher.while_started() => {
                TaskNextAction::Stop
            },

            block = self.blocks_events.next() => {
                if let Some(block) = block {
                    let block_id = {
                        let block: SharedImportResult = block;
                        block.sealed_block.entity.id()
                    };
                    let mut account = self.account_metadata.take().expect("Account metadata is not set; qed");
                    let next_order = self.prev_order.wrapping_add(1);

                    let result = self.shared_sequencer_client.send(self.signer.as_ref(), account, next_order, block_id).await;

                    match result {
                        Ok(_) => {
                            account.sequence = account.sequence.saturating_add(1);
                            self.prev_order = next_order;
                            self.account_metadata = Some(account);
                            TaskNextAction::Continue
                        }
                        Err(err) => {
                            TaskNextAction::ErrorContinue(err)
                        }
                    }
                } else {
                    TaskNextAction::Stop
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
) -> ServiceRunner<NonInitializedTask<S>>
where
    B: BlocksProvider,
    S: Signer,
{
    let blocks_events = block_provider.subscribe();
    ServiceRunner::new(NonInitializedTask::new(config, blocks_events, signer))
}
