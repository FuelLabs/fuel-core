use std::ops::Deref;

use fuel_core_global_merkle_root_storage::update::UpdateMerkleizedTables;
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
    TaskNextAction,
};
use fuel_core_storage::transactional::WriteTransaction;
use fuel_core_types::fuel_types::ChainId;

use crate::ports::{
    BlockStream,
    ServiceStorage,
};

/// Service definition as a thin wrapper over `ServiceRunner`.
pub struct Service<B, S>(ServiceRunner<UpdateMerkleRootTask<B, S>>)
where
    B: BlockStream + Send + 'static,
    B::Error: std::error::Error + Send + Sync + 'static,
    S: ServiceStorage + Send + 'static;

impl<B, S> Service<B, S>
where
    B: BlockStream + Send + 'static,
    B::Error: std::error::Error + Send + Sync + 'static,
    S: ServiceStorage + Send + 'static,
{
    /// Construct a new service.
    pub fn new(chain_id: ChainId, storage: S, blocks: B) -> Self {
        Self(ServiceRunner::new(UpdateMerkleRootTask::new(
            chain_id, storage, blocks,
        )))
    }
}

impl<B, S> Deref for Service<B, S>
where
    B: BlockStream + Send + 'static,
    B::Error: std::error::Error + Send + Sync + 'static,
    S: ServiceStorage + Send + 'static,
{
    type Target = ServiceRunner<UpdateMerkleRootTask<B, S>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// The inner task definition. Holds the state of the service.
pub struct UpdateMerkleRootTask<BlockStream, Storage> {
    chain_id: ChainId,
    storage: Storage,
    blocks: BlockStream,
}

impl<B, S> UpdateMerkleRootTask<B, S> {
    pub fn new(chain_id: ChainId, storage: S, blocks: B) -> Self {
        Self {
            chain_id,
            storage,
            blocks,
        }
    }
}

#[async_trait::async_trait]
impl<B, S> RunnableService for UpdateMerkleRootTask<B, S>
where
    B: BlockStream + Send,
    B::Error: std::error::Error + Send + Sync + 'static,
    S: ServiceStorage + Send,
{
    const NAME: &'static str = "MerkleRootService";

    type SharedData = ();

    type Task = Self;

    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        ()
    }

    async fn into_task(
        self,
        _state_watcher: &StateWatcher,
        _params: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        Ok(self)
    }
}

impl<B, S> RunnableTask for UpdateMerkleRootTask<B, S>
where
    B: BlockStream + Send,
    B::Error: std::error::Error + Send + Sync + 'static,
    S: ServiceStorage + Send,
{
    #[tracing::instrument(skip(self, watcher))]
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        tokio::select! {
            biased;
            _ = watcher.while_started() => {
                TaskNextAction::Stop
            }
            _ = self.process_next_block() => {
                TaskNextAction::Continue
            }
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}

impl<B, S> UpdateMerkleRootTask<B, S>
where
    B: BlockStream,
    B::Error: std::error::Error + Send + Sync + 'static,
    S: ServiceStorage,
{
    #[tracing::instrument(skip(self))]
    async fn process_next_block(&mut self) -> anyhow::Result<()> {
        let block = self.blocks.next().await?;
        let mut tx = self.storage.write_transaction();

        tx.update_merkleized_tables(self.chain_id, &block)?;

        tx.commit()?;

        Ok(())
    }
}
