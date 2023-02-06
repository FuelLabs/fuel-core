//! Service utilities for running fuel sync.
use std::sync::Arc;

use crate::{
    import::{
        Config,
        Import,
    },
    ports::{
        self,
        BlockImporterPort,
        ConsensusPort,
        PeerToPeerPort,
    },
    state::State,
    sync::SyncHeights,
};

use fuel_core_services::{
    stream::{
        BoxStream,
        IntoBoxStream,
    },
    RunnableService,
    RunnableTask,
    Service,
    ServiceRunner,
    SharedMutex,
    StateWatcher,
};
use fuel_core_types::blockchain::primitives::BlockHeight;
use futures::StreamExt;
use tokio::sync::Notify;

#[cfg(test)]
mod tests;

/// Creates an instance of runnable sync service.
pub fn new_service<P, E, C>(
    current_fuel_block_height: BlockHeight,
    p2p: P,
    executor: E,
    consensus: C,
    params: Config,
) -> anyhow::Result<ServiceRunner<SyncTask<P, E, C>>>
where
    P: ports::PeerToPeerPort + Send + Sync + 'static,
    E: ports::BlockImporterPort + Send + Sync + 'static,
    C: ports::ConsensusPort + Send + Sync + 'static,
{
    let height_stream = p2p.height_stream();
    let committed_height_stream = executor.committed_height_stream();
    let state = State::new(Some(current_fuel_block_height.into()), None);
    Ok(ServiceRunner::new(SyncTask::new(
        height_stream,
        committed_height_stream,
        state,
        params,
        p2p,
        executor,
        consensus,
    )?))
}

/// Task for syncing heights.
/// Contains import task as a child task.
pub struct SyncTask<P, E, C>
where
    P: PeerToPeerPort + Send + Sync + 'static,
    E: BlockImporterPort + Send + Sync + 'static,
    C: ConsensusPort + Send + Sync + 'static,
{
    sync_heights: SyncHeights,
    import_task_handle: ServiceRunner<ImportTask<P, E, C>>,
}

struct ImportTask<P, E, C>(Import<P, E, C>);

impl<P, E, C> SyncTask<P, E, C>
where
    P: PeerToPeerPort + Send + Sync + 'static,
    E: BlockImporterPort + Send + Sync + 'static,
    C: ConsensusPort + Send + Sync + 'static,
{
    fn new(
        height_stream: BoxStream<BlockHeight>,
        committed_height_stream: BoxStream<BlockHeight>,
        state: State,
        params: Config,
        p2p: P,
        executor: E,
        consensus: C,
    ) -> anyhow::Result<Self> {
        let notify = Arc::new(Notify::new());
        let state = SharedMutex::new(state);
        let p2p = Arc::new(p2p);
        let executor = Arc::new(executor);
        let consensus = Arc::new(consensus);
        let sync_heights = SyncHeights::new(
            height_stream,
            committed_height_stream,
            state.clone(),
            notify.clone(),
        );
        let import = Import::new(state, notify, params, p2p, executor, consensus);
        let import_task_handle = ServiceRunner::new(ImportTask(import));
        Ok(Self {
            sync_heights,
            import_task_handle,
        })
    }
}

#[async_trait::async_trait]
impl<P, E, C> RunnableTask for SyncTask<P, E, C>
where
    P: PeerToPeerPort + Send + Sync + 'static,
    E: BlockImporterPort + Send + Sync + 'static,
    C: ConsensusPort + Send + Sync + 'static,
{
    #[tracing::instrument(level = "debug", skip_all, err, ret)]
    async fn run(
        &mut self,
        _: &mut fuel_core_services::StateWatcher,
    ) -> anyhow::Result<bool> {
        if self.import_task_handle.state().stopped() {
            return Ok(false)
        }
        Ok(self.sync_heights.sync().await.is_some())
    }
}

#[async_trait::async_trait]
impl<P, E, C> RunnableService for SyncTask<P, E, C>
where
    P: PeerToPeerPort + Send + Sync + 'static,
    E: BlockImporterPort + Send + Sync + 'static,
    C: ConsensusPort + Send + Sync + 'static,
{
    const NAME: &'static str = "fuel-core-sync";

    type SharedData = ();

    type Task = SyncTask<P, E, C>;

    fn shared_data(&self) -> Self::SharedData {}

    async fn into_task(mut self, watcher: &StateWatcher) -> anyhow::Result<Self::Task> {
        let mut watcher = watcher.clone();
        self.sync_heights.map_stream(|height_stream| {
            height_stream
                .take_until(async move {
                    let _ = watcher.while_started().await;
                })
                .into_boxed()
        });
        self.import_task_handle.start_and_await().await?;

        Ok(self)
    }
}

#[async_trait::async_trait]
impl<P, E, C> RunnableTask for ImportTask<P, E, C>
where
    P: PeerToPeerPort + Send + Sync + 'static,
    E: BlockImporterPort + Send + Sync + 'static,
    C: ConsensusPort + Send + Sync + 'static,
{
    #[tracing::instrument(level = "debug", skip_all, err, ret)]
    async fn run(
        &mut self,
        watcher: &mut fuel_core_services::StateWatcher,
    ) -> anyhow::Result<bool> {
        self.0.import(watcher).await
    }
}

#[async_trait::async_trait]
impl<P, E, C> RunnableService for ImportTask<P, E, C>
where
    P: PeerToPeerPort + Send + Sync + 'static,
    E: BlockImporterPort + Send + Sync + 'static,
    C: ConsensusPort + Send + Sync + 'static,
{
    const NAME: &'static str = "fuel-core-sync/import-task";

    type SharedData = ();

    type Task = ImportTask<P, E, C>;

    fn shared_data(&self) -> Self::SharedData {}

    async fn into_task(self, _: &StateWatcher) -> anyhow::Result<Self::Task> {
        Ok(self)
    }
}

impl<P, E, C> Drop for SyncTask<P, E, C>
where
    P: PeerToPeerPort + Send + Sync + 'static,
    E: BlockImporterPort + Send + Sync + 'static,
    C: ConsensusPort + Send + Sync + 'static,
{
    fn drop(&mut self) {
        tracing::info!("Sync task shutting down");
        self.import_task_handle.stop();
    }
}
