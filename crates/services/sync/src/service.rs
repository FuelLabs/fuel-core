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
    Service,
    ServiceRunner,
    SharedMutex,
    StateWatcher,
};
use fuel_core_types::fuel_types::BlockHeight;
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
    Ok(ServiceRunner::new(
        SyncTask::new(
            height_stream,
            committed_height_stream,
            state,
            params,
            p2p,
            executor,
            consensus,
        )?,
        (),
    ))
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
        let import_task_handle = ServiceRunner::new(ImportTask(import), ());
        Ok(Self {
            sync_heights,
            import_task_handle,
        })
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
    type Params = ();

    fn shared_data(&self) -> Self::SharedData {}

    async fn start(
        mut self,
        watcher: &StateWatcher,
        _: Self::Params,
    ) -> anyhow::Result<Self> {
        let mut sync_watcher = watcher.clone();
        self.import_task_handle.start_and_await().await?;
        let mut import_watcher = self.import_task_handle.state_watcher();
        self.sync_heights.map_stream(|height_stream| {
            height_stream
                .take_until(async move {
                    tokio::select! {
                        _ = sync_watcher.while_started() => {},
                        _ = import_watcher.while_started() => {},
                    }
                })
                .into_boxed()
        });

        Ok(self)
    }

    #[tracing::instrument(level = "debug", skip_all, err, ret)]
    async fn run(&mut self, _: &mut StateWatcher) -> anyhow::Result<bool> {
        Ok(self.sync_heights.sync().await.is_some())
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        tracing::info!("Sync task shutting down");
        self.import_task_handle.stop_and_await().await?;
        Ok(())
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
    type Params = ();

    fn shared_data(&self) -> Self::SharedData {}

    async fn start(self, _: &StateWatcher, _: Self::Params) -> anyhow::Result<Self> {
        Ok(self)
    }

    #[tracing::instrument(level = "debug", skip_all, err, ret)]
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        self.0.import(watcher).await
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        // Nothing to shut down because we don't have any temporary state that should be dumped,
        // and we don't spawn any sub-tasks that we need to finish or await.
        Ok(())
    }
}
