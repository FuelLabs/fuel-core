//! Service utilities for running fuel sync.
use std::sync::Arc;

use crate::{
    import::{
        import,
        Config,
    },
    ports::{
        self,
        BlockImporterPort,
        ConsensusPort,
        PeerToPeerPort,
        Ports,
    },
    state::State,
    sync::sync,
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
    let state = State::new(*current_fuel_block_height, None);
    let ports = ports::Ports {
        p2p: Arc::new(p2p),
        executor: Arc::new(executor),
        consensus: Arc::new(consensus),
    };
    let import_task = ImportTask::new(state, params, ports);

    Ok(ServiceRunner::new(SyncTask::new(
        height_stream,
        import_task,
    )?))
}

#[derive(Debug, Clone)]
struct SharedTaskData {
    state: SharedMutex<State>,
    notify: Arc<Notify>,
}

/// Task for syncing heights.
/// Contains import task as a child task.
pub struct SyncTask<P, E, C>
where
    P: PeerToPeerPort + Send + Sync + 'static,
    E: BlockImporterPort + Send + Sync + 'static,
    C: ConsensusPort + Send + Sync + 'static,
{
    height_stream: BoxStream<BlockHeight>,
    shared: SharedTaskData,
    import_task_handle: ServiceRunner<ImportTask<P, E, C>>,
}

struct ImportTask<P, E, C>
where
    P: PeerToPeerPort + Send + Sync + 'static,
    E: BlockImporterPort + Send + Sync + 'static,
    C: ConsensusPort + Send + Sync + 'static,
{
    shared: SharedTaskData,
    params: Config,
    ports: Ports<P, E, C>,
}

impl<P, E, C> SyncTask<P, E, C>
where
    P: PeerToPeerPort + Send + Sync + 'static,
    E: BlockImporterPort + Send + Sync + 'static,
    C: ConsensusPort + Send + Sync + 'static,
{
    fn new(
        height_stream: BoxStream<BlockHeight>,
        import_task: ImportTask<P, E, C>,
    ) -> anyhow::Result<Self> {
        let shared = import_task.shared_data();
        let import_task_handle = ServiceRunner::new(import_task);
        import_task_handle.start()?;
        Ok(Self {
            height_stream,
            shared,
            import_task_handle,
        })
    }
}

impl<P, E, C> ImportTask<P, E, C>
where
    P: PeerToPeerPort + Send + Sync + 'static,
    E: BlockImporterPort + Send + Sync + 'static,
    C: ConsensusPort + Send + Sync + 'static,
{
    fn new(state: State, params: Config, ports: Ports<P, E, C>) -> Self {
        let shared = SharedTaskData {
            state: SharedMutex::new(state),
            notify: Arc::new(Notify::new()),
        };
        Self {
            shared,
            params,
            ports,
        }
    }
}

#[async_trait::async_trait]
impl<P, E, C> RunnableTask for SyncTask<P, E, C>
where
    P: PeerToPeerPort + Send + Sync + 'static,
    E: BlockImporterPort + Send + Sync + 'static,
    C: ConsensusPort + Send + Sync + 'static,
{
    async fn run(
        &mut self,
        _: &mut fuel_core_services::StateWatcher,
    ) -> anyhow::Result<bool> {
        if self.import_task_handle.state().stopped() {
            return Ok(false)
        }
        Ok(sync(
            &mut self.height_stream,
            &self.shared.state,
            &self.shared.notify,
        )
        .await
        .is_some())
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
        let height_stream = core::mem::replace(
            &mut self.height_stream,
            futures::stream::pending().into_boxed(),
        );
        let height_stream = height_stream
            .take_until(async move {
                let _ = watcher.while_started().await;
            })
            .into_boxed();
        self.height_stream = height_stream;

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
    async fn run(
        &mut self,
        watcher: &mut fuel_core_services::StateWatcher,
    ) -> anyhow::Result<bool> {
        import(
            &self.shared.state,
            &self.shared.notify,
            self.params,
            &self.ports,
            watcher,
        )
        .await
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

    type SharedData = SharedTaskData;

    type Task = ImportTask<P, E, C>;

    fn shared_data(&self) -> Self::SharedData {
        self.shared.clone()
    }

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
        self.import_task_handle.stop();
    }
}
