use crate::{
    deadline_clock::{
        DeadlineClock,
        OnConflict,
    },
    ports::{
        BlockImporter,
        BlockProducer,
        TransactionPool,
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
    fuel_tx::{
        ConsensusParameters,
        UniqueIdentifier,
    },
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
        txpool::TxStatus,
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
use tracing::error;

pub type Service<T, B, I> = ServiceRunner<Task<T, B, I>>;

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

pub struct Task<T, B, I> {
    block_gas_limit: Word,
    signing_key: Option<Secret<SecretKeyWrapper>>,
    block_producer: B,
    block_importer: I,
    txpool: T,
    tx_status_update_stream: BoxStream<TxStatus>,
    request_receiver: mpsc::Receiver<Request>,
    shared_state: SharedState,
    last_height: BlockHeight,
    last_timestamp: Tai64,
    last_block_created: Instant,
    trigger: Trigger,
    // TODO: Consider that the creation of the block takes some time, and maybe we need to
    //  patch the timer to generate the block earlier.
    //  https://github.com/FuelLabs/fuel-core/issues/918
    /// Deadline clock, used by the triggers
    timer: DeadlineClock,
    consensus_params: ConsensusParameters,
}

impl<T, B, I> Task<T, B, I>
where
    T: TransactionPool,
{
    pub fn new(
        last_block: &BlockHeader,
        config: Config,
        txpool: T,
        block_producer: B,
        block_importer: I,
    ) -> Self {
        let tx_status_update_stream = txpool.transaction_status_events();
        let (request_sender, request_receiver) = mpsc::channel(100);
        let last_timestamp = last_block.time();
        let duration =
            Duration::from_secs(Tai64::now().0.saturating_sub(last_timestamp.0));
        let last_block_created = Instant::now() - duration;
        Self {
            block_gas_limit: config.block_gas_limit,
            signing_key: config.signing_key,
            txpool,
            block_producer,
            block_importer,
            tx_status_update_stream,
            request_receiver,
            shared_state: SharedState { request_sender },
            last_height: *last_block.height(),
            last_timestamp,
            last_block_created,
            trigger: config.trigger,
            timer: DeadlineClock::new(),
            consensus_params: config.consensus_params,
        }
    }

    fn next_height(&self) -> BlockHeight {
        self.last_height + 1u32.into()
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
                Trigger::Hybrid { min_block_time, .. } => {
                    increase_time(self.last_timestamp, min_block_time)
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

impl<D, T, B, I> Task<T, B, I>
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
        let mut block_time = block_production
            .start_time
            .unwrap_or(self.next_time(RequestType::Manual)?);
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
        let produce_block_start = Instant::now();
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
        for (tx, err) in skipped_transactions {
            error!(
                "During block production got invalid transaction {:?} with error {:?}",
                tx, err
            );
            tx_ids_to_remove.push(tx.id(&self.consensus_params));
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
            ImportResult {
                sealed_block: block,
                tx_status,
            },
            db_transaction,
        ))?;

        // Update last block time
        self.last_height = height;
        self.last_timestamp = block_time;
        self.last_block_created = Instant::now();

        // Set timer for the next block
        match (self.trigger, request_type) {
            (Trigger::Never, RequestType::Manual) => (),
            (Trigger::Never, RequestType::Trigger) => {
                unreachable!("Trigger production will never produce blocks in never mode")
            }
            (Trigger::Instant, _) => {}
            (Trigger::Interval { block_time }, RequestType::Trigger) => {
                self.timer
                    .set_deadline(produce_block_start + block_time, OnConflict::Min)
                    .await;
            }
            (Trigger::Interval { block_time }, RequestType::Manual) => {
                self.timer
                    .set_deadline(produce_block_start + block_time, OnConflict::Overwrite)
                    .await;
            }
            (
                Trigger::Hybrid {
                    max_block_time,
                    min_block_time,
                    max_tx_idle_time,
                },
                RequestType::Trigger,
            ) => {
                let consumable_gas = self.txpool.total_consumable_gas();

                // If txpool still has more than a full block of transactions available,
                // produce new block in min_block_time.
                if consumable_gas > self.block_gas_limit {
                    self.timer
                        .set_timeout(min_block_time, OnConflict::Max)
                        .await;
                } else if self.txpool.pending_number() > 0 {
                    // If we still have available txs, reduce the timeout to max idle time
                    self.timer
                        .set_timeout(max_tx_idle_time, OnConflict::Max)
                        .await;
                } else {
                    self.timer
                        .set_timeout(max_block_time, OnConflict::Max)
                        .await;
                }
            }
            (Trigger::Hybrid { .. }, RequestType::Manual) => {
                unreachable!("Trigger types hybrid cannot be used with manual. This is enforced during config validation")
            }
        }

        Ok(())
    }

    pub(crate) async fn on_txpool_event(
        &mut self,
        txpool_event: TxStatus,
    ) -> anyhow::Result<()> {
        match txpool_event {
            TxStatus::Submitted => match self.trigger {
                Trigger::Instant => {
                    let pending_number = self.txpool.pending_number();
                    // skip production if there are no pending transactions
                    if pending_number > 0 {
                        self.produce_next_block().await?;
                    }
                    Ok(())
                }
                Trigger::Never | Trigger::Interval { .. } => Ok(()),
                Trigger::Hybrid {
                    max_tx_idle_time,
                    min_block_time,
                    ..
                } => {
                    let consumable_gas = self.txpool.total_consumable_gas();

                    // If we have over one full block of transactions and min_block_time
                    // has expired, start block production immediately
                    if consumable_gas > self.block_gas_limit
                        && self.last_block_created + min_block_time < Instant::now()
                    {
                        self.produce_next_block().await?;
                    } else if self.txpool.pending_number() > 0 {
                        // We have at least one transaction, so tx_max_idle_time is the limit
                        self.timer
                            .set_timeout(max_tx_idle_time, OnConflict::Min)
                            .await;
                    }

                    Ok(())
                }
            },
            TxStatus::Completed => Ok(()), // This has been processed already
            TxStatus::SqueezedOut { .. } => {
                // TODO: If this is the only tx, set timer deadline to last_block_time + max_block_time
                Ok(())
            }
        }
    }

    async fn on_timer(&mut self, _at: Instant) -> anyhow::Result<()> {
        match self.trigger {
            Trigger::Instant | Trigger::Never => {
                unreachable!("Timer is never set in this mode");
            }
            // In the Interval mode the timer expires only when a new block should be created.
            // In the Hybrid mode the timer can be either:
            // 1. min_block_time expired after it was set when a block
            //    would have been produced too soon
            // 2. max_tx_idle_time expired after a tx has arrived
            // 3. max_block_time expired
            // => we produce a new block in any case
            Trigger::Interval { .. } | Trigger::Hybrid { .. } => {
                self.produce_next_block().await?;
                Ok(())
            }
        }
    }
}

#[async_trait::async_trait]
impl<T, B, I> RunnableService for Task<T, B, I>
where
    Self: RunnableTask,
{
    const NAME: &'static str = "PoA";

    type SharedData = SharedState;
    type Task = Task<T, B, I>;

    fn shared_data(&self) -> Self::SharedData {
        self.shared_state.clone()
    }

    async fn into_task(self, _: &StateWatcher) -> anyhow::Result<Self::Task> {
        match self.trigger {
            Trigger::Never | Trigger::Instant => {}
            Trigger::Interval { block_time } => {
                self.timer
                    .set_timeout(block_time, OnConflict::Overwrite)
                    .await;
            }
            Trigger::Hybrid { max_block_time, .. } => {
                self.timer
                    .set_timeout(max_block_time, OnConflict::Overwrite)
                    .await;
            }
        };
        Ok(self)
    }
}

#[async_trait::async_trait]
impl<D, T, B, I> RunnableTask for Task<T, B, I>
where
    T: TransactionPool,
    B: BlockProducer<Database = D>,
    I: BlockImporter<Database = D>,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        let should_continue;
        tokio::select! {
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
                if let Some(txpool_event) = txpool_event {
                    self.on_txpool_event(txpool_event).await.context("While processing txpool event")?;
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
        // Nothing to shut down because we don't have any temporary state that should be dumped,
        // and we don't spawn any sub-tasks that we need to finish or await.
        Ok(())
    }
}

pub fn new_service<D, T, B, I>(
    last_block: &BlockHeader,
    config: Config,
    txpool: T,
    block_producer: B,
    block_importer: I,
) -> Service<T, B, I>
where
    T: TransactionPool + 'static,
    B: BlockProducer<Database = D> + 'static,
    I: BlockImporter<Database = D> + 'static,
{
    Service::new(Task::new(
        last_block,
        config,
        txpool,
        block_producer,
        block_importer,
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
