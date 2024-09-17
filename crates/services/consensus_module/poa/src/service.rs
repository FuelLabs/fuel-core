use anyhow::{
    anyhow,
    Context,
};
use std::{
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::{
        mpsc,
        oneshot,
    },
    time::{
        sleep_until,
        Instant,
    },
};
use tokio_stream::StreamExt;

use crate::{
    ports::{
        BlockImporter,
        BlockProducer,
        BlockSigner,
        GetTime,
        P2pPort,
        PredefinedBlocks,
        TransactionPool,
        TransactionsSource,
    },
    sync::{
        SyncState,
        SyncTask,
    },
    Config,
    Trigger,
};
use fuel_core_services::{
    stream::{
        BoxFuture,
        BoxStream,
    },
    RunnableService,
    RunnableTask,
    Service as OtherService,
    ServiceRunner,
    StateWatcher,
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

pub type Service<T, B, I, S, PB, C> = ServiceRunner<MainTask<T, B, I, S, PB, C>>;

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

pub struct MainTask<T, B, I, S, PB, C> {
    signer: S,
    block_producer: B,
    block_importer: I,
    txpool: T,
    tx_status_update_stream: BoxStream<TxId>,
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
}

impl<T, B, I, S, PB, C> MainTask<T, B, I, S, PB, C>
where
    T: TransactionPool,
    I: BlockImporter,
    PB: PredefinedBlocks,
    C: GetTime,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new<P: P2pPort>(
        last_block: &BlockHeader,
        config: Config,
        txpool: T,
        block_producer: B,
        block_importer: I,
        p2p_port: P,
        signer: S,
        predefined_blocks: PB,
        clock: C,
    ) -> Self {
        let tx_status_update_stream = txpool.transaction_status_events();
        let (request_sender, request_receiver) = mpsc::channel(1024);
        let (last_height, last_timestamp, last_block_created) =
            Self::extract_block_info(clock.now(), last_block);

        let block_stream = block_importer.block_stream();
        let peer_connections_stream = p2p_port.reserved_peers_count();

        let Config {
            min_connected_reserved_peers,
            time_until_synced,
            trigger,
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

        Self {
            signer,
            txpool,
            block_producer,
            block_importer,
            tx_status_update_stream,
            request_receiver,
            shared_state: SharedState { request_sender },
            last_height,
            last_timestamp,
            last_block_created,
            predefined_blocks,
            trigger,
            sync_task_handle,
            clock,
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
            },
            RequestType::Trigger => {
                let now = self.clock.now();
                if now > self.last_timestamp {
                    Ok(now)
                } else {
                    self.next_time(RequestType::Manual)
                }
            }
        }
    }
}

impl<T, B, I, S, PB, C> MainTask<T, B, I, S, PB, C>
where
    T: TransactionPool,
    B: BlockProducer,
    I: BlockImporter,
    S: BlockSigner,
    PB: PredefinedBlocks,
    C: GetTime,
{
    // Request the block producer to make a new block, and return it when ready
    async fn signal_produce_block(
        &self,
        height: BlockHeight,
        block_time: Tai64,
        source: TransactionsSource,
    ) -> anyhow::Result<UncommittedExecutionResult<Changes>> {
        self.block_producer
            .produce_and_execute_block(height, block_time, source)
            .await
    }

    pub(crate) async fn produce_next_block(&mut self) -> anyhow::Result<()> {
        self.produce_block(
            self.next_height(),
            self.next_time(RequestType::Trigger)?,
            TransactionsSource::TxPool,
        )
        .await
    }

    async fn produce_manual_blocks(
        &mut self,
        block_production: ManualProduction,
    ) -> anyhow::Result<()> {
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
            .signal_produce_block(height, block_time, source)
            .await?
            .into();

        let mut tx_ids_to_remove = Vec::with_capacity(skipped_transactions.len());
        for (tx_id, err) in skipped_transactions {
            tracing::error!(
                "During block production got invalid transaction {:?} with error {:?}",
                tx_id,
                err
            );
            tx_ids_to_remove.push((tx_id, err));
        }
        self.txpool.remove_txs(tx_ids_to_remove);

        // Sign the block and seal it
        let seal = self.signer.seal_block(&block).await?;
        let block = SealedBlock {
            entity: block,
            consensus: seal,
        };

        block.entity.header().time();

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
        self.last_block_created = last_block_created;

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

    pub(crate) async fn on_txpool_event(&mut self) -> anyhow::Result<()> {
        match self.trigger {
            Trigger::Instant => {
                let pending_number = self.txpool.pending_number();
                // skip production if there are no pending transactions
                if pending_number > 0 {
                    self.produce_next_block().await?;
                }
                Ok(())
            }
            Trigger::Never | Trigger::Interval { .. } => Ok(()),
        }
    }

    async fn on_timer(&mut self) -> anyhow::Result<()> {
        match self.trigger {
            Trigger::Instant | Trigger::Never => {
                unreachable!("Timer is never set in this mode");
            }
            // In the Interval mode the timer expires only when a new block should be created.
            Trigger::Interval { .. } => {
                self.produce_next_block().await?;
                Ok(())
            }
        }
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
}

#[derive(Serialize)]
struct PredefinedBlockWithSkippedTransactions {
    block: Block,
    skipped_transactions: Vec<(TxId, ExecutorError)>,
}

#[async_trait::async_trait]
impl<T, B, I, S, PB, C> RunnableService for MainTask<T, B, I, S, PB, C>
where
    Self: RunnableTask,
{
    const NAME: &'static str = "PoA";

    type SharedData = SharedState;
    type Task = MainTask<T, B, I, S, PB, C>;
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
            Trigger::Interval { .. } => {
                return Ok(Self {
                    last_block_created: Instant::now(),
                    ..self
                })
            }
        }

        Ok(self)
    }
}

#[async_trait::async_trait]
impl<T, B, I, S, PB, C> RunnableTask for MainTask<T, B, I, S, PB, C>
where
    T: TransactionPool,
    B: BlockProducer,
    I: BlockImporter,
    S: BlockSigner,
    PB: PredefinedBlocks,
    C: GetTime,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        let should_continue;
        let mut sync_state = self.sync_task_handle.shared.clone();
        // make sure we're synced first
        while *sync_state.borrow_and_update() == SyncState::NotSynced {
            tokio::select! {
                biased;
                result = watcher.while_started() => {
                    should_continue = result?.started();
                    return Ok(should_continue);
                }
                _ = sync_state.changed() => {
                    break;
                }
                _ = self.tx_status_update_stream.next() => {
                    // ignore txpool events while syncing
                }
            }
        }

        if let SyncState::Synced(block_header) = &*sync_state.borrow_and_update() {
            self.update_last_block_values(block_header);
        }

        let next_height = self.next_height();
        let maybe_block = self.predefined_blocks.get_block(&next_height)?;
        if let Some(block) = maybe_block {
            self.produce_predefined_block(&block).await?;
            should_continue = true;
            return Ok(should_continue)
        }

        let next_block_production: BoxFuture<()> = match self.trigger {
            Trigger::Never | Trigger::Instant => Box::pin(core::future::pending()),
            Trigger::Interval { block_time } => Box::pin(sleep_until(
                self.last_block_created
                    .checked_add(block_time)
                    .ok_or(anyhow!("Time exceeds system limits"))?,
            )),
        };

        tokio::select! {
            biased;
            _ = watcher.while_started() => {
                should_continue = false;
            }
            request = self.request_receiver.recv() => {
                if let Some(request) = request {
                    match request {
                        Request::ManualBlocks((block, response)) => {
                            let result = self.produce_manual_blocks(block).await;
                            let _ = response.send(result);
                        }
                    }
                    should_continue = true;
                } else {
                    tracing::error!("The PoA task should be the holder of the `Sender`");
                    should_continue = false;
                }
            }
            // TODO: This should likely be refactored to use something like tokio::sync::Notify.
            //       Otherwise, if a bunch of txs are submitted at once and all the txs are included
            //       into the first block production trigger, we'll still call the event handler
            //       for each tx after they've already been included into a block.
            //       The poa service also doesn't care about events unrelated to new tx submissions,
            //       and shouldn't be awoken when txs are completed or squeezed out of the pool.
            txpool_event = self.tx_status_update_stream.next() => {
                if txpool_event.is_some()  {
                    self.on_txpool_event().await.context("While processing txpool event")?;
                    should_continue = true;
                } else {
                    should_continue = false;
                }
            }
            _ = next_block_production => {
                match self.on_timer().await.context("While processing timer event") {
                    Ok(()) => should_continue = true,
                    Err(err) => {
                        // Wait some time in case of error to avoid spamming retry block production
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        return Err(err);
                    }
                };
            }
        }

        Ok(should_continue)
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        tracing::info!("PoA MainTask shutting down");
        self.sync_task_handle.stop_and_await().await?;
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
pub fn new_service<T, B, I, P, S, PB, C>(
    last_block: &BlockHeader,
    config: Config,
    txpool: T,
    block_producer: B,
    block_importer: I,
    p2p_port: P,
    block_signer: S,
    predefined_blocks: PB,
    clock: C,
) -> Service<T, B, I, S, PB, C>
where
    T: TransactionPool + 'static,
    B: BlockProducer + 'static,
    I: BlockImporter + 'static,
    S: BlockSigner + 'static,
    PB: PredefinedBlocks + 'static,
    P: P2pPort,
    C: GetTime,
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
