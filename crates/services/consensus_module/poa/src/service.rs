use anyhow::anyhow;
use std::{
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::{
        mpsc,
        oneshot,
    },
    time::Instant,
};

use crate::{
    ports::{
        BlockImporter,
        BlockProducer,
        BlockProductionReadySignal,
        BlockSigner,
        GetTime,
        P2pPort,
        PredefinedBlocks,
        TransactionPool,
        TransactionsSource,
        WaitForReadySignal,
    },
    sync::{
        SyncState,
        SyncTask,
    },
    Config,
    Trigger,
};
use fuel_core_services::{
    stream::BoxFuture,
    RunnableService,
    RunnableTask,
    Service as OtherService,
    ServiceRunner,
    StateWatcher,
    TaskNextAction,
};
use fuel_core_storage::transactional::Changes;
use fuel_core_types::{
    blockchain::{
        block::Block,
        header::BlockHeader,
        SealedBlock,
    },
    fuel_tx::{
        Transaction,
        TxId,
    },
    fuel_types::BlockHeight,
    services::{
        block_importer::ImportResult,
        executor::{
            Error as ExecutorError,
            ExecutionResult,
            UncommittedResult as UncommittedExecutionResult,
        },
        Uncommitted,
    },
    tai64::Tai64,
};
use serde::Serialize;
use tokio::time::sleep_until;

pub type Service<B, I, S, PB, C, RS> = ServiceRunner<MainTask<B, I, S, PB, C, RS>>;

#[derive(Clone)]
pub struct SharedState {
    request_sender: mpsc::Sender<Request>,
}

impl SharedState {
    pub async fn manually_produce_block(
        &self,
        start_time: Option<Tai64>,
        mode: Mode,
    ) -> anyhow::Result<()> {
        let (sender, receiver) = oneshot::channel();

        self.request_sender
            .send(Request::ManualBlocks((
                ManualProduction { start_time, mode },
                sender,
            )))
            .await?;
        receiver.await?
    }
}

pub enum Mode {
    /// Produces `number_of_blocks` blocks using `TxPool` as a source of transactions.
    Blocks { number_of_blocks: u32 },
    /// Produces one block with the given transactions.
    BlockWithTransactions(Vec<Transaction>),
}

struct ManualProduction {
    pub start_time: Option<Tai64>,
    pub mode: Mode,
}

/// Requests accepted by the task.
enum Request {
    /// Manually produces the next blocks with `Tai64` block timestamp.
    /// The block timestamp should be higher than previous one.
    ManualBlocks((ManualProduction, oneshot::Sender<anyhow::Result<()>>)),
}

impl core::fmt::Debug for Request {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Request")
    }
}

pub(crate) enum RequestType {
    Manual,
    Trigger,
}

pub struct MainTask<B, I, S, PB, C, RS> {
    signer: Arc<S>,
    block_producer: B,
    block_importer: I,
    new_txs_watcher: tokio::sync::watch::Receiver<()>,
    request_receiver: mpsc::Receiver<Request>,
    shared_state: SharedState,
    last_height: BlockHeight,
    last_timestamp: Tai64,
    last_block_created: Instant,
    predefined_blocks: PB,
    trigger: Trigger,
    clock: C,
    /// Deadline clock, used by the triggers
    sync_task_handle: ServiceRunner<SyncTask>,
    production_timeout: Duration,
    /// externally controlled start of block production
    block_production_ready_signal: BlockProductionReadySignal<RS>,
}

impl<B, I, S, PB, C, RS> MainTask<B, I, S, PB, C, RS>
where
    I: BlockImporter,
    PB: PredefinedBlocks,
    C: GetTime,
    RS: WaitForReadySignal,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new<P: P2pPort, T: TransactionPool>(
        last_block: &BlockHeader,
        config: Config,
        txpool: T,
        block_producer: B,
        block_importer: I,
        p2p_port: P,
        signer: Arc<S>,
        predefined_blocks: PB,
        clock: C,
        block_production_ready_signal: RS,
    ) -> Self {
        let new_txs_watcher = txpool.new_txs_watcher();
        let (request_sender, request_receiver) = mpsc::channel(1024);
        let (last_height, last_timestamp, last_block_created) =
            Self::extract_block_info(clock.now(), last_block);

        let block_stream = block_importer.block_stream();
        let peer_connections_stream = p2p_port.reserved_peers_count();

        let Config {
            min_connected_reserved_peers,
            time_until_synced,
            trigger,
            production_timeout,
            ..
        } = config;

        let sync_task = SyncTask::new(
            peer_connections_stream,
            min_connected_reserved_peers,
            time_until_synced,
            block_stream,
            last_block,
        );

        let sync_task_handle = ServiceRunner::new(sync_task);

        let block_production_ready_signal =
            BlockProductionReadySignal::new(block_production_ready_signal);

        Self {
            signer,
            block_producer,
            block_importer,
            new_txs_watcher,
            request_receiver,
            shared_state: SharedState { request_sender },
            last_height,
            last_timestamp,
            last_block_created,
            predefined_blocks,
            trigger,
            sync_task_handle,
            clock,
            production_timeout,
            block_production_ready_signal,
        }
    }

    fn extract_block_info(
        now: Tai64,
        last_block: &BlockHeader,
    ) -> (BlockHeight, Tai64, Instant) {
        let last_timestamp = last_block.time();
        let duration_since_last_block =
            Duration::from_secs(now.0.saturating_sub(last_timestamp.0));
        let last_block_created = Instant::now()
            .checked_sub(duration_since_last_block)
            .unwrap_or(Instant::now());
        let last_height = *last_block.height();
        (last_height, last_timestamp, last_block_created)
    }

    fn next_height(&self) -> BlockHeight {
        self.last_height
            .succ()
            .expect("It should be impossible to produce more blocks than u32::MAX")
    }

    fn next_time(&self, request_type: RequestType) -> anyhow::Result<Tai64> {
        match request_type {
            RequestType::Manual => match self.trigger {
                Trigger::Never | Trigger::Instant => {
                    let duration = self.last_block_created.elapsed();
                    increase_time(self.last_timestamp, duration)
                }
                Trigger::Interval { block_time } => {
                    increase_time(self.last_timestamp, block_time)
                }
                Trigger::Open { period } => increase_time(self.last_timestamp, period),
            },
            RequestType::Trigger => {
                let now = self.clock.now();
                match self.trigger {
                    Trigger::Open { period } => {
                        let expected_timestamp =
                            increase_time(self.last_timestamp, period)?;
                        if now > expected_timestamp {
                            Ok(now)
                        } else {
                            Ok(expected_timestamp)
                        }
                    }
                    _ => {
                        if now > self.last_timestamp {
                            Ok(now)
                        } else {
                            self.next_time(RequestType::Manual)
                        }
                    }
                }
            }
        }
    }
}

impl<B, I, S, PB, C, RS> MainTask<B, I, S, PB, C, RS>
where
    B: BlockProducer,
    I: BlockImporter,
    S: BlockSigner,
    PB: PredefinedBlocks,
    C: GetTime,
    RS: WaitForReadySignal,
{
    // Request the block producer to make a new block, and return it when ready
    async fn signal_produce_block(
        &self,
        height: BlockHeight,
        block_time: Tai64,
        source: TransactionsSource,
        deadline: Instant,
    ) -> anyhow::Result<UncommittedExecutionResult<Changes>> {
        let future = self
            .block_producer
            .produce_and_execute_block(height, block_time, source, deadline);

        let result = tokio::time::timeout(self.production_timeout, future)
            .await
            .map_err(|_| anyhow::anyhow!("Block production timed out"))??;

        // In the case if the block production finished before the deadline
        // we need to wait until the deadline is reached to guarantee
        // the correct interval between blocks
        sleep_until(deadline).await;

        Ok(result)
    }

    pub(crate) async fn produce_next_block(
        &mut self,
        deadline: Instant,
    ) -> anyhow::Result<()> {
        self.produce_block(
            self.next_height(),
            self.next_time(RequestType::Trigger)?,
            TransactionsSource::TxPool,
            deadline,
        )
        .await
    }

    async fn produce_manual_blocks(
        &mut self,
        block_production: ManualProduction,
    ) -> anyhow::Result<()> {
        // The caller of manual block production expects that it is resolved immediately
        // while in the case of `Trigger::Open` we need to wait for the period to pass.
        if matches!(self.trigger, Trigger::Open { .. }) {
            return Err(anyhow!(
                "Manual block production is not allowed with trigger `Open`"
            ))
        }

        let mut block_time = if let Some(time) = block_production.start_time {
            time
        } else {
            self.next_time(RequestType::Manual)?
        };
        match block_production.mode {
            Mode::Blocks { number_of_blocks } => {
                for _ in 0..number_of_blocks {
                    self.produce_block(
                        self.next_height(),
                        block_time,
                        TransactionsSource::TxPool,
                        Instant::now(),
                    )
                    .await?;
                    block_time = self.next_time(RequestType::Manual)?;
                }
            }
            Mode::BlockWithTransactions(txs) => {
                self.produce_block(
                    self.next_height(),
                    block_time,
                    TransactionsSource::SpecificTransactions(txs),
                    Instant::now(),
                )
                .await?;
            }
        }
        Ok(())
    }

    async fn produce_block(
        &mut self,
        height: BlockHeight,
        block_time: Tai64,
        source: TransactionsSource,
        deadline: Instant,
    ) -> anyhow::Result<()> {
        let last_block_created = Instant::now();
        // verify signing key is set
        if !self.signer.is_available() {
            return Err(anyhow!("unable to produce blocks without a consensus key"))
        }

        if self.last_timestamp > block_time {
            return Err(anyhow!("The block timestamp should monotonically increase"))
        }
        // Ask the block producer to create the block
        let (
            ExecutionResult {
                block,
                skipped_transactions,
                tx_status,
                events,
            },
            changes,
        ) = self
            .signal_produce_block(height, block_time, source, deadline)
            .await?
            .into();

        if !skipped_transactions.is_empty() {
            for (tx_id, err) in skipped_transactions {
                tracing::error!(
                    "During block production got invalid transaction {:?} with error {:?}",
                    tx_id,
                    err
                );
            }
        }

        // Sign the block and seal it
        let seal = self.signer.seal_block(&block).await?;
        let block = SealedBlock {
            entity: block,
            consensus: seal,
        };

        // Import the sealed block
        self.block_importer
            .commit_result(Uncommitted::new(
                ImportResult::new_from_local(block, tx_status, events),
                changes,
            ))
            .await?;

        // Update last block time
        self.last_height = height;
        self.last_timestamp = block_time;
        self.last_block_created = match self.trigger {
            Trigger::Open { .. } => deadline.max(last_block_created),
            _ => last_block_created,
        };

        Ok(())
    }

    async fn produce_predefined_block(
        &mut self,
        predefined_block: &Block,
    ) -> anyhow::Result<()> {
        tracing::info!("Producing predefined block");
        let last_block_created = Instant::now();
        if !self.signer.is_available() {
            return Err(anyhow!("unable to produce blocks without a signer"))
        }

        // Ask the block producer to create the block
        let (
            ExecutionResult {
                block,
                skipped_transactions,
                tx_status,
                events,
            },
            changes,
        ) = self
            .block_producer
            .produce_predefined_block(predefined_block)
            .await?
            .into();

        if !skipped_transactions.is_empty() {
            let block_and_skipped = PredefinedBlockWithSkippedTransactions {
                block: predefined_block.clone(),
                skipped_transactions,
            };
            let serialized = serde_json::to_string_pretty(&block_and_skipped)?;
            tracing::error!(
                "During block production got invalid transactions: BEGIN {} END",
                serialized
            );
        }

        // Sign the block and seal it
        let seal = self.signer.seal_block(&block).await?;
        let sealed_block = SealedBlock {
            entity: block,
            consensus: seal,
        };
        // Import the sealed block
        self.block_importer
            .commit_result(Uncommitted::new(
                ImportResult::new_from_local(sealed_block.clone(), tx_status, events),
                changes,
            ))
            .await?;

        // Update last block time
        self.last_height = *sealed_block.entity.header().height();
        self.last_timestamp = sealed_block.entity.header().time();
        self.last_block_created = last_block_created;

        Ok(())
    }

    fn update_last_block_values(&mut self, block_header: &Arc<BlockHeader>) {
        let (last_height, last_timestamp, last_block_created) =
            Self::extract_block_info(self.clock.now(), block_header);
        if last_height > self.last_height {
            self.last_height = last_height;
            self.last_timestamp = last_timestamp;
            self.last_block_created = last_block_created;
        }
    }

    async fn ensure_synced(
        &mut self,
        watcher: &mut StateWatcher,
    ) -> Option<TaskNextAction> {
        let mut sync_state = self.sync_task_handle.shared.clone();

        if *sync_state.borrow_and_update() == SyncState::NotSynced {
            tokio::select! {
                biased;
                result = watcher.while_started() => {
                    return Some(result.map(|state| state.started()).into())
                }
                _ = sync_state.changed() => {}
            }
        }

        if let SyncState::Synced(block_header) = &*sync_state.borrow_and_update() {
            self.update_last_block_values(block_header);
        }
        None
    }

    async fn maybe_produce_predefined_block(&mut self) -> Option<TaskNextAction> {
        let next_height = self.next_height();
        let maybe_block = match self.predefined_blocks.get_block(&next_height) {
            Ok(option) => option,
            Err(err) => return Some(TaskNextAction::ErrorContinue(err)),
        };
        if let Some(block) = maybe_block {
            let res = self.produce_predefined_block(&block).await;
            return match res {
                Ok(()) => Some(TaskNextAction::Continue),
                Err(err) => Some(TaskNextAction::ErrorContinue(err)),
            }
        }
        None
    }

    async fn handle_requested_production(
        &mut self,
        request: Option<Request>,
    ) -> TaskNextAction {
        if let Some(request) = request {
            match request {
                Request::ManualBlocks((block, response)) => {
                    let result = self.produce_manual_blocks(block).await;
                    let _ = response.send(result);
                }
            }
            TaskNextAction::Continue
        } else {
            tracing::error!("The PoA task should be the holder of the `Sender`");
            TaskNextAction::Stop
        }
    }

    async fn handle_normal_block_production(
        &mut self,
        deadline: Instant,
    ) -> TaskNextAction {
        match self.produce_next_block(deadline).await {
            Ok(()) => TaskNextAction::Continue,
            Err(err) => {
                // Wait some time in case of error to avoid spamming retry block production
                tokio::time::sleep(Duration::from_secs(1)).await;
                TaskNextAction::ErrorContinue(err)
            }
        }
    }
}

#[derive(Serialize)]
struct PredefinedBlockWithSkippedTransactions {
    block: Block,
    skipped_transactions: Vec<(TxId, ExecutorError)>,
}

#[async_trait::async_trait]
impl<B, I, S, PB, C, RS> RunnableService for MainTask<B, I, S, PB, C, RS>
where
    Self: RunnableTask,
    RS: WaitForReadySignal,
{
    const NAME: &'static str = "PoA";

    type SharedData = SharedState;
    type Task = MainTask<B, I, S, PB, C, RS>;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        self.shared_state.clone()
    }

    async fn into_task(
        self,
        _: &StateWatcher,
        _: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        self.sync_task_handle.start_and_await().await?;

        match self.trigger {
            Trigger::Never | Trigger::Instant => {}
            Trigger::Interval { .. } | Trigger::Open { .. } => {
                return Ok(Self {
                    last_block_created: Instant::now(),
                    ..self
                })
            }
        }

        Ok(self)
    }
}

impl<B, I, S, PB, C, RS> RunnableTask for MainTask<B, I, S, PB, C, RS>
where
    B: BlockProducer,
    I: BlockImporter,
    S: BlockSigner,
    PB: PredefinedBlocks,
    C: GetTime,
    RS: WaitForReadySignal,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        tokio::select! {
            biased;
            _ = watcher.while_started() => {
                return TaskNextAction::Stop
            }
            _ = self.block_production_ready_signal.wait_for_ready_signal() => {}
        }

        if let Some(action) = self.ensure_synced(watcher).await {
            return action;
        }

        if let Some(action) = self.maybe_produce_predefined_block().await {
            return action;
        }

        let next_block_production: BoxFuture<Instant> = match self.trigger {
            Trigger::Never => Box::pin(core::future::pending::<Instant>()),
            Trigger::Instant => Box::pin(async {
                let _ = self.new_txs_watcher.changed().await;
                Instant::now()
            }),
            Trigger::Interval { block_time } => {
                let next_block_time = match self
                    .last_block_created
                    .checked_add(block_time)
                    .ok_or(anyhow!("Time exceeds system limits"))
                {
                    Ok(time) => time,
                    Err(err) => return TaskNextAction::ErrorContinue(err),
                };
                Box::pin(async move {
                    sleep_until(next_block_time).await;
                    Instant::now()
                })
            }
            Trigger::Open { period } => {
                let deadline = match self
                    .last_block_created
                    .checked_add(period)
                    .ok_or(anyhow!("Time exceeds system limits"))
                {
                    Ok(time) => time,
                    Err(err) => return TaskNextAction::ErrorContinue(err),
                };
                Box::pin(async move { deadline })
            }
        };

        tokio::select! {
            biased;
            _ = watcher.while_started() => {
                TaskNextAction::Stop
            }
            request = self.request_receiver.recv() => {
                self.handle_requested_production(request).await
            }
            deadline = next_block_production => {
                self.handle_normal_block_production(deadline).await
            }
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        tracing::info!("PoA MainTask shutting down");
        self.sync_task_handle.stop_and_await().await?;
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
pub fn new_service<T, B, I, P, S, PB, C, RS>(
    last_block: &BlockHeader,
    config: Config,
    txpool: T,
    block_producer: B,
    block_importer: I,
    p2p_port: P,
    block_signer: Arc<S>,
    predefined_blocks: PB,
    clock: C,
    block_production_ready_signal: RS,
) -> Service<B, I, S, PB, C, RS>
where
    T: TransactionPool + 'static,
    B: BlockProducer + 'static,
    I: BlockImporter + 'static,
    S: BlockSigner + 'static,
    PB: PredefinedBlocks + 'static,
    P: P2pPort,
    C: GetTime,
    RS: WaitForReadySignal,
{
    Service::new(MainTask::new(
        last_block,
        config,
        txpool,
        block_producer,
        block_importer,
        p2p_port,
        block_signer,
        predefined_blocks,
        clock,
        block_production_ready_signal,
    ))
}

fn increase_time(time: Tai64, duration: Duration) -> anyhow::Result<Tai64> {
    let timestamp = time.0;
    let timestamp = timestamp
        .checked_add(duration.as_secs())
        .ok_or(anyhow::anyhow!(
            "The provided time parameters lead to an overflow"
        ))?;
    Ok(Tai64(timestamp))
}
