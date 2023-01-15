use crate::{
    deadline_clock::{
        DeadlineClock,
        OnConflict,
    },
    ports::{
        BlockDb,
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
    EmptyShared,
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
        primitives::{
            BlockHeight,
            SecretKeyWrapper,
        },
        SealedBlock,
    },
    fuel_asm::Word,
    fuel_crypto::Signature,
    fuel_tx::UniqueIdentifier,
    secrecy::{
        ExposeSecret,
        Secret,
    },
    services::{
        executor::{
            ExecutionResult,
            UncommittedResult,
        },
        txpool::TxStatus,
    },
};
use std::ops::Deref;
use tokio::{
    sync::broadcast,
    time::Instant,
};
use tokio_stream::StreamExt;
use tracing::error;

pub type Service<D, T, B> = ServiceRunner<Task<D, T, B>>;

pub struct Task<D, T, B> {
    pub(crate) block_gas_limit: Word,
    pub(crate) signing_key: Option<Secret<SecretKeyWrapper>>,
    pub(crate) db: D,
    pub(crate) block_producer: B,
    pub(crate) txpool: T,
    pub(crate) tx_status_update_stream: BoxStream<TxStatus>,
    pub(crate) import_block_events_tx: broadcast::Sender<SealedBlock>,
    /// Last block creation time. When starting up, this is initialized
    /// to `Instant::now()`, which delays the first block on startup for
    /// a bit, but doesn't cause any other issues.
    pub(crate) last_block_created: Instant,
    pub(crate) trigger: Trigger,
    /// Deadline clock, used by the triggers
    pub(crate) timer: DeadlineClock,
}

impl<D, T, B> Task<D, T, B>
where
    T: TransactionPool,
{
    pub fn new(
        config: Config,
        txpool: T,
        import_block_events_tx: broadcast::Sender<SealedBlock>,
        block_producer: B,
        db: D,
    ) -> Self {
        let tx_status_update_stream = txpool.transaction_status_events();
        Self {
            block_gas_limit: config.block_gas_limit,
            signing_key: config.signing_key,
            db,
            block_producer,
            txpool,
            tx_status_update_stream,
            last_block_created: Instant::now(),
            import_block_events_tx,
            trigger: config.trigger,
            timer: DeadlineClock::new(),
        }
    }
}

impl<D, T, B> Task<D, T, B>
where
    D: BlockDb,
    T: TransactionPool,
    B: BlockProducer<D>,
{
    // Request the block producer to make a new block, and return it when ready
    async fn signal_produce_block(
        &self,
    ) -> anyhow::Result<UncommittedResult<StorageTransaction<D>>> {
        let current_height = self
            .db
            .block_height()
            .map_err(|err| anyhow::format_err!("db error {err:?}"))?;
        let height = BlockHeight::from(current_height.as_usize() + 1);

        self.block_producer
            .produce_and_execute_block(height, self.block_gas_limit)
            .await
    }

    pub(crate) async fn produce_block(&mut self) -> anyhow::Result<()> {
        // verify signing key is set
        if self.signing_key.is_none() {
            return Err(anyhow!("unable to produce blocks without a consensus key"))
        }

        // Ask the block producer to create the block
        let (
            ExecutionResult {
                block,
                skipped_transactions,
                ..
            },
            mut db_transaction,
        ) = self.signal_produce_block().await?.into();

        // sign the block and seal it
        let seal = seal_block(&self.signing_key, &block, db_transaction.as_mut())?;
        db_transaction.commit()?;

        let mut tx_ids_to_remove = Vec::with_capacity(skipped_transactions.len());
        for (tx, err) in skipped_transactions {
            error!(
                "During block production got invalid transaction {:?} with error {:?}",
                tx, err
            );
            tx_ids_to_remove.push(tx.id());
        }
        self.txpool.remove_txs(tx_ids_to_remove);

        // Send the block back to the txpool
        // TODO: this probably must be done differently with multi-node configuration
        let sealed_block = SealedBlock {
            entity: block,
            consensus: seal,
        };
        self.import_block_events_tx
            .send(sealed_block)
            .expect("Failed to import the generated block");

        // Update last block time
        self.last_block_created = Instant::now();

        // Set timer for the next block
        match self.trigger {
            Trigger::Never => {
                unreachable!("This mode will never produce blocks");
            }
            Trigger::Instant => {}
            Trigger::Interval { block_time } => {
                // TODO: instead of sleeping for `block_time`, subtract the time we used for processing
                self.timer.set_timeout(block_time, OnConflict::Min).await;
            }
            Trigger::Hybrid {
                max_block_time,
                min_block_time,
                max_tx_idle_time,
            } => {
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
                        self.produce_block().await?;
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
                        self.produce_block().await?;
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
                self.produce_block().await?;
                Ok(())
            }
        }
    }
}

#[async_trait::async_trait]
impl<D, T, B> RunnableService for Task<D, T, B>
where
    Self: RunnableTask,
{
    const NAME: &'static str = "PoA";

    type SharedData = EmptyShared;
    type Task = Task<D, T, B>;

    fn shared_data(&self) -> Self::SharedData {
        EmptyShared
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
impl<D, T, B> RunnableTask for Task<D, T, B>
where
    D: BlockDb,
    T: TransactionPool,
    B: BlockProducer<D>,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        let should_continue;
        tokio::select! {
            _ = watcher.while_started() => {
                should_continue = false;
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
}

pub fn new_service<D, T, B>(
    config: Config,
    txpool: T,
    import_block_events_tx: broadcast::Sender<SealedBlock>,
    block_producer: B,
    db: D,
) -> Service<D, T, B>
where
    T: TransactionPool + 'static,
    D: BlockDb + 'static,
    B: BlockProducer<D> + 'static,
{
    Service::new(Task::new(
        config,
        txpool,
        import_block_events_tx,
        block_producer,
        db,
    ))
}

pub fn seal_block(
    signing_key: &Option<Secret<SecretKeyWrapper>>,
    block: &Block,
    database: &mut dyn BlockDb,
) -> anyhow::Result<Consensus> {
    if let Some(key) = signing_key {
        let block_hash = block.id();
        let message = block_hash.into_message();

        // The length of the secret is checked
        let signing_key = key.expose_secret().deref();

        let poa_signature = Signature::sign(signing_key, &message);
        let seal = Consensus::PoA(PoAConsensus::new(poa_signature));
        database.seal_block(block_hash, seal.clone())?;
        Ok(seal)
    } else {
        Err(anyhow!("no PoA signing key configured"))
    }
}
