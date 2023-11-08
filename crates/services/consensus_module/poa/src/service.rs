use crate::{
    deadline_clock::{
        DeadlineClock,
        OnConflict,
    },
    ports::{
        BlockImporter,
        BlockProducer,
        P2pPort,
        TransactionPool,
    },
    sync::{
        SyncState,
        SyncTask,
    },
    Config,
    Trigger,
};
use anyhow::{
    anyhow,
    Context,
};
use fuel_core_services::{
    stream::BoxStream,
    RunnableService,
    RunnableTask,
    Service as _,
    ServiceRunner,
    StateWatcher,
};
use fuel_core_storage::transactional::StorageTransaction;
use fuel_core_types::{
    blockchain::{
        block::Block,
        consensus::{
            poa::PoAConsensus,
            Consensus,
        },
        header::BlockHeader,
        primitives::SecretKeyWrapper,
        SealedBlock,
    },
    fuel_asm::Word,
    fuel_crypto::Signature,
    fuel_tx::TxId,
    fuel_types::BlockHeight,
    secrecy::{
        ExposeSecret,
        Secret,
    },
    services::{
        block_importer::ImportResult,
        executor::{
            ExecutionResult,
            UncommittedResult as UncommittedExecutionResult,
        },
        Uncommitted,
    },
    tai64::Tai64,
};
use std::{
    ops::Deref,
    time::Duration,
};
use tokio::{
    sync::{
        mpsc,
        oneshot,
    },
    time::Instant,
};
use tokio_stream::StreamExt;

pub type Service<T, B, I> = ServiceRunner<MainTask<T, B, I>>;
#[derive(Clone)]
pub struct SharedState {
    request_sender: mpsc::Sender<Request>,
}

impl SharedState {
    pub async fn manually_produce_block(
        &self,
        start_time: Option<Tai64>,
        number_of_blocks: u32,
    ) -> anyhow::Result<()> {
        let (sender, receiver) = oneshot::channel();

        self.request_sender
            .send(Request::ManualBlocks((
                ManualProduction {
                    start_time,
                    number_of_blocks,
                },
                sender,
            )))
            .await?;
        receiver.await?
    }
}

struct ManualProduction {
    pub start_time: Option<Tai64>,
    pub number_of_blocks: u32,
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

pub struct MainTask<T, B, I> {
    block_gas_limit: Word,
    signing_key: Option<Secret<SecretKeyWrapper>>,
    block_producer: B,
    block_importer: I,
    txpool: T,
    tx_status_update_stream: BoxStream<TxId>,
    request_receiver: mpsc::Receiver<Request>,
    shared_state: SharedState,
    last_height: BlockHeight,
    last_timestamp: Tai64,
    last_block_created: Instant,
    trigger: Trigger,
    /// Deadline clock, used by the triggers
    timer: DeadlineClock,
    sync_task_handle: ServiceRunner<SyncTask>,
}

impl<T, B, I> MainTask<T, B, I>
where
    T: TransactionPool,
    I: BlockImporter,
{
    pub fn new<P: P2pPort>(
        last_block: &BlockHeader,
        config: Config,
        txpool: T,
        block_producer: B,
        block_importer: I,
        p2p_port: P,
    ) -> Self {
        let tx_status_update_stream = txpool.transaction_status_events();
        let (request_sender, request_receiver) = mpsc::channel(1024);
        let (last_height, last_timestamp, last_block_created) =
            Self::extract_block_info(last_block);

        let block_stream = block_importer.block_stream();
        let peer_connections_stream = p2p_port.reserved_peers_count();

        let Config {
            block_gas_limit,
            signing_key,
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
            block_gas_limit,
            signing_key,
            txpool,
            block_producer,
            block_importer,
            tx_status_update_stream,
            request_receiver,
            shared_state: SharedState { request_sender },
            last_height,
            last_timestamp,
            last_block_created,
            trigger,
            timer: DeadlineClock::new(),
            sync_task_handle,
        }
    }

    fn extract_block_info(last_block: &BlockHeader) -> (BlockHeight, Tai64, Instant) {
        let last_timestamp = last_block.time();
        let duration =
            Duration::from_secs(Tai64::now().0.saturating_sub(last_timestamp.0));
        let last_block_created = Instant::now()
            .checked_sub(duration)
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
                let now = Tai64::now();
                if now > self.last_timestamp {
                    Ok(now)
                } else {
                    self.next_time(RequestType::Manual)
                }
            }
        }
    }
}

impl<D, T, B, I> MainTask<T, B, I>
where
    T: TransactionPool,
    B: BlockProducer<Database = D>,
    I: BlockImporter<Database = D>,
{
    // Request the block producer to make a new block, and return it when ready
    async fn signal_produce_block(
        &self,
        height: BlockHeight,
        block_time: Tai64,
    ) -> anyhow::Result<UncommittedExecutionResult<StorageTransaction<D>>> {
        self.block_producer
            .produce_and_execute_block(height, block_time, self.block_gas_limit)
            .await
    }

    pub(crate) async fn produce_next_block(&mut self) -> anyhow::Result<()> {
        self.produce_block(
            self.next_height(),
            self.next_time(RequestType::Trigger)?,
            RequestType::Trigger,
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
        for _ in 0..block_production.number_of_blocks {
            self.produce_block(self.next_height(), block_time, RequestType::Manual)
                .await?;
            block_time = self.next_time(RequestType::Manual)?;
        }
        Ok(())
    }

    async fn produce_block(
        &mut self,
        height: BlockHeight,
        block_time: Tai64,
        request_type: RequestType,
    ) -> anyhow::Result<()> {
        let last_block_created = Instant::now();
        // verify signing key is set
        if self.signing_key.is_none() {
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
            },
            db_transaction,
        ) = self.signal_produce_block(height, block_time).await?.into();

        let mut tx_ids_to_remove = Vec::with_capacity(skipped_transactions.len());
        for (tx_id, err) in skipped_transactions {
            tracing::error!(
                "During block production got invalid transaction {:?} with error {:?}",
                tx_id,
                err
            );
            tx_ids_to_remove.push(tx_id);
        }
        self.txpool.remove_txs(tx_ids_to_remove);

        // Sign the block and seal it
        let seal = seal_block(&self.signing_key, &block)?;
        let block = SealedBlock {
            entity: block,
            consensus: seal,
        };
        // Import the sealed block
        self.block_importer.commit_result(Uncommitted::new(
            ImportResult::new_from_local(block, tx_status),
            db_transaction,
        ))?;

        // Update last block time
        self.last_height = height;
        self.last_timestamp = block_time;
        self.last_block_created = last_block_created;

        // Set timer for the next block
        match (self.trigger, request_type) {
            (Trigger::Never, RequestType::Manual) => (),
            (Trigger::Never, RequestType::Trigger) => {
                unreachable!("Trigger production will never produce blocks in never mode")
            }
            (Trigger::Instant, _) => {}
            (Trigger::Interval { block_time }, RequestType::Trigger) => {
                let deadline = last_block_created.checked_add(block_time).expect("It is impossible to overflow except in the case where we don't want to produce a block.");
                self.timer.set_deadline(deadline, OnConflict::Min).await;
            }
            (Trigger::Interval { block_time }, RequestType::Manual) => {
                let deadline = last_block_created.checked_add(block_time).expect("It is impossible to overflow except in the case where we don't want to produce a block.");
                self.timer
                    .set_deadline(deadline, OnConflict::Overwrite)
                    .await;
            }
        }

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

    async fn on_timer(&mut self, _at: Instant) -> anyhow::Result<()> {
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
}

#[async_trait::async_trait]
impl<T, B, I> RunnableService for MainTask<T, B, I>
where
    Self: RunnableTask,
{
    const NAME: &'static str = "PoA";

    type SharedData = SharedState;
    type Task = MainTask<T, B, I>;
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
            Trigger::Interval { block_time } => {
                self.timer
                    .set_timeout(block_time, OnConflict::Overwrite)
                    .await;
            }
        };

        Ok(self)
    }
}

#[async_trait::async_trait]
impl<D, T, B, I> RunnableTask for MainTask<T, B, I>
where
    T: TransactionPool,
    B: BlockProducer<Database = D>,
    I: BlockImporter<Database = D>,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        let should_continue;
        // make sure we're synced first
        while *self.sync_task_handle.shared.borrow() == SyncState::NotSynced {
            tokio::select! {
                biased;
                result = watcher.while_started() => {
                    should_continue = result?.started();
                    return Ok(should_continue);
                }
                _ = self.sync_task_handle.shared.changed() => {
                    if let SyncState::Synced(block_header) = &*self.sync_task_handle.shared.borrow() {
                        let (last_height, last_timestamp, last_block_created) =
                            Self::extract_block_info(block_header);
                        self.last_height = last_height;
                        self.last_timestamp = last_timestamp;
                        self.last_block_created = last_block_created;
                    }
                }
            }
        }

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
                    unreachable!("The task is the holder of the `Sender` too")
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
            at = self.timer.wait() => {
                self.on_timer(at).await.context("While processing timer event")?;
                should_continue = true;
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

pub fn new_service<D, T, B, I, P>(
    last_block: &BlockHeader,
    config: Config,
    txpool: T,
    block_producer: B,
    block_importer: I,
    p2p_port: P,
) -> Service<T, B, I>
where
    T: TransactionPool + 'static,
    B: BlockProducer<Database = D> + 'static,
    I: BlockImporter<Database = D> + 'static,
    P: P2pPort,
{
    Service::new(MainTask::new(
        last_block,
        config,
        txpool,
        block_producer,
        block_importer,
        p2p_port,
    ))
}

fn seal_block(
    signing_key: &Option<Secret<SecretKeyWrapper>>,
    block: &Block,
) -> anyhow::Result<Consensus> {
    if let Some(key) = signing_key {
        let block_hash = block.id();
        let message = block_hash.into_message();

        // The length of the secret is checked
        let signing_key = key.expose_secret().deref();

        let poa_signature = Signature::sign(signing_key, &message);
        let seal = Consensus::PoA(PoAConsensus::new(poa_signature));
        Ok(seal)
    } else {
        Err(anyhow!("no PoA signing key configured"))
    }
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
