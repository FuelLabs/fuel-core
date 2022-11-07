use crate::{
    deadline_clock::{
        DeadlineClock,
        OnConflict,
    },
    Config,
    Trigger,
};
use anyhow::{
    anyhow,
    Context,
};
use fuel_core_interfaces::{
    block_importer::ImportBlockBroadcast,
    block_producer::BlockProducer,
    common::{
        fuel_tx::UniqueIdentifier,
        prelude::{
            Signature,
            Word,
        },
        secrecy::{
            ExposeSecret,
            Secret,
        },
    },
    executor::ExecutionResult,
    model::{
        BlockHeight,
        FuelBlock,
        FuelBlockConsensus,
        FuelBlockPoAConsensus,
        SecretKeyWrapper,
    },
    poa_coordinator::{
        BlockDb,
        TransactionPool,
    },
    txpool::{
        TxStatus,
        TxStatusBroadcast,
    },
};
use parking_lot::Mutex;
use std::{
    ops::Deref,
    sync::Arc,
};
use tokio::{
    sync::{
        broadcast,
        mpsc,
    },
    task::JoinHandle,
    time::Instant,
};
use tracing::{
    error,
    warn,
};

pub struct RunningService {
    join: JoinHandle<()>,
    stop: mpsc::Sender<()>,
}

pub struct Service {
    running: Mutex<Option<RunningService>>,
    config: Config,
}

impl Service {
    pub fn new(config: &Config) -> Self {
        Self {
            running: Mutex::new(None),
            config: config.clone(),
        }
    }

    pub async fn start<S, T>(
        &self,
        txpool_broadcast: broadcast::Receiver<TxStatusBroadcast>,
        txpool: T,
        import_block_events_tx: broadcast::Sender<ImportBlockBroadcast>,
        block_producer: Arc<dyn BlockProducer>,
        db: S,
    ) where
        S: BlockDb + Send + Clone + 'static,
        T: TransactionPool + Send + Sync + 'static,
    {
        let mut running = self.running.lock();

        if running.is_some() {
            warn!("Trying to start a service that is already running");
            return
        }

        let (stop_tx, stop_rx) = mpsc::channel(1);

        let task = Task {
            stop: stop_rx,
            block_gas_limit: self.config.block_gas_limit,
            signing_key: self.config.signing_key.clone(),
            db,
            block_producer,
            txpool_broadcast,
            txpool,
            last_block_created: Instant::now(),
            import_block_events_tx,
            trigger: self.config.trigger,
            timer: DeadlineClock::new(),
        };

        *running = Some(RunningService {
            join: tokio::spawn(task.run()),
            stop: stop_tx,
        });
    }

    pub async fn stop(&self) -> Option<JoinHandle<()>> {
        let maybe_running = self.running.lock().take();
        if let Some(running) = maybe_running {
            // Ignore possible send error, as the JoinHandle will report errors anyway
            let _ = running.stop.send(()).await;
            Some(running.join)
        } else {
            warn!("Trying to stop a service that is not running");
            None
        }
    }
}

pub struct Task<S, T>
where
    S: BlockDb + Send + Sync,
    T: TransactionPool,
{
    stop: mpsc::Receiver<()>,
    block_gas_limit: Word,
    signing_key: Option<Secret<SecretKeyWrapper>>,
    db: S,
    block_producer: Arc<dyn BlockProducer>,
    txpool: T,
    txpool_broadcast: broadcast::Receiver<TxStatusBroadcast>,
    import_block_events_tx: broadcast::Sender<ImportBlockBroadcast>,
    /// Last block creation time. When starting up, this is initialized
    /// to `Instant::now()`, which delays the first block on startup for
    /// a bit, but doesn't cause any other issues.
    last_block_created: Instant,
    trigger: Trigger,
    /// Deadline clock, used by the triggers
    timer: DeadlineClock,
}
impl<S, T> Task<S, T>
where
    S: BlockDb + Send,
    T: TransactionPool,
{
    // Request the block producer to make a new block, and return it when ready
    async fn signal_produce_block(&mut self) -> anyhow::Result<ExecutionResult> {
        let current_height = self
            .db
            .block_height()
            .map_err(|err| anyhow::format_err!("db error {err:?}"))?;
        let height = BlockHeight::from(current_height.as_usize() + 1);

        self.block_producer
            .produce_and_execute_block(height, self.block_gas_limit)
            .await
    }

    async fn produce_block(&mut self) -> anyhow::Result<()> {
        // verify signing key is set
        if self.signing_key.is_none() {
            return Err(anyhow!("unable to produce blocks without a consensus key"))
        }

        // Ask the block producer to create the block
        let ExecutionResult {
            block,
            skipped_transactions,
        } = self.signal_produce_block().await?;

        // sign the block and seal it
        self.seal_block(&block)?;

        let mut tx_ids_to_remove = Vec::with_capacity(skipped_transactions.len());
        for (tx, err) in skipped_transactions {
            error!(
                "During block production got invalid transaction {:?} with error {:?}",
                tx, err
            );
            tx_ids_to_remove.push(tx.id());
        }

        if let Err(err) = self.txpool.remove_txs(tx_ids_to_remove).await {
            error!(
                "Unable to clean up skipped transaction from `TxPool` with error {:?}",
                err
            );
        };

        // Send the block back to the txpool
        // TODO: this probably must be done differently with multi-node configuration
        self.import_block_events_tx
            .send(ImportBlockBroadcast::PendingFuelBlockImported {
                block: Arc::new(block),
            })
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
                self.timer
                    .set_timeout(max_block_time, OnConflict::Min)
                    .await;

                let consumable_gas = self.txpool.total_consumable_gas().await?;

                // If txpool still has more than a full block of transactions available,
                // produce new block in min_block_time.
                if consumable_gas > self.block_gas_limit {
                    self.timer
                        .set_timeout(min_block_time, OnConflict::Min)
                        .await;
                } else if consumable_gas > 0 {
                    // If we still have available txs, reduce the timeout to max idle time
                    self.timer
                        .set_timeout(max_tx_idle_time, OnConflict::Min)
                        .await;
                }
            }
        }

        Ok(())
    }

    async fn on_txpool_event(
        &mut self,
        txpool_event: &TxStatusBroadcast,
    ) -> anyhow::Result<()> {
        match txpool_event.status {
            TxStatus::Submitted => match self.trigger {
                Trigger::Instant => {
                    self.produce_block().await?;
                    Ok(())
                }
                Trigger::Never | Trigger::Interval { .. } => Ok(()),
                Trigger::Hybrid {
                    max_tx_idle_time,
                    min_block_time,
                    ..
                } => {
                    let consumable_gas = self.txpool.total_consumable_gas().await?;

                    // If we have over one full block of transactions and min_block_time
                    // has expired, start block production immediately
                    if consumable_gas > self.block_gas_limit
                        && self.last_block_created + min_block_time < Instant::now()
                    {
                        self.produce_block().await?;
                    } else {
                        // We have at least one transaction, so tx_max_idle_time is the limit
                        self.timer
                            .set_timeout(max_tx_idle_time, OnConflict::Min)
                            .await;
                    }

                    Ok(())
                }
            },
            TxStatus::Executed => Ok(()), // This has been processed already
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

    /// Processes the next incoming event. Called by the main event loop.
    /// Returns Ok(false) if the event loop should stop.
    async fn process_next_event(&mut self) -> anyhow::Result<bool> {
        tokio::select! {
            _ = self.stop.recv() => {
                Ok(false)
            }
            txpool_event = self.txpool_broadcast.recv() => {
                self.on_txpool_event(&txpool_event.context("Broadcast receive error")?).await.context("While processing txpool event")?;
                Ok(true)
            }
            at = self.timer.wait() => {
                self.on_timer(at).await.context("While processing timer event")?;
                Ok(true)
            }
        }
    }

    async fn init_timers(&mut self) {
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
        }
    }

    fn seal_block(&mut self, block: &FuelBlock) -> anyhow::Result<()> {
        if let Some(key) = &self.signing_key {
            let block_hash = block.id();
            let message = block_hash.into_message();

            // The length of the secret is checked
            let signing_key = key.expose_secret().deref();

            let poa_signature = Signature::sign(signing_key, &message);
            let seal = FuelBlockConsensus::PoA(FuelBlockPoAConsensus::new(poa_signature));
            self.db.seal_block(block_hash, seal)
        } else {
            Err(anyhow!("no PoA signing key configured"))
        }
    }

    /// Start event loop
    async fn run(mut self) {
        self.init_timers().await;
        loop {
            match self.process_next_event().await {
                Ok(should_continue) => {
                    if !should_continue {
                        break
                    }
                }
                Err(err) => {
                    error!("PoA module encountered an error: {err:?}");
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use fuel_core_interfaces::{
        common::{
            fuel_crypto::SecretKey,
            fuel_tx::{
                Receipt,
                Transaction,
                TransactionBuilder,
                TxId,
            },
        },
        executor::Error,
        model::{
            ArcPoolTx,
            BlockId,
        },
    };
    use rand::{
        prelude::StdRng,
        Rng,
        SeedableRng,
    };
    use std::collections::HashSet;

    struct MockBlockProducer {
        skipped_transactions: Vec<Transaction>,
    }

    #[async_trait::async_trait]
    impl BlockProducer for MockBlockProducer {
        async fn produce_and_execute_block(
            &self,
            _height: BlockHeight,
            _max_gas: Word,
        ) -> anyhow::Result<ExecutionResult> {
            let result = ExecutionResult {
                block: Default::default(),
                skipped_transactions: self
                    .skipped_transactions
                    .clone()
                    .into_iter()
                    .map(|tx| (tx, Error::OutputAlreadyExists))
                    .collect(),
            };
            Ok(result)
        }

        async fn dry_run(
            &self,
            _transaction: Transaction,
            _height: Option<BlockHeight>,
            _utxo_validation: Option<bool>,
        ) -> anyhow::Result<Vec<Receipt>> {
            unimplemented!()
        }
    }

    mockall::mock! {
        TxPool {}

        #[async_trait::async_trait]
        impl TransactionPool for TxPool {
            async fn total_consumable_gas(&self) -> anyhow::Result<u64>;

            async fn remove_txs(&mut self, tx_ids: Vec<TxId>) -> anyhow::Result<Vec<ArcPoolTx>>;
        }
    }

    mockall::mock! {
        Database {}

        unsafe impl Sync for Database {}
        unsafe impl Send for Database {}

        #[async_trait::async_trait]
        impl BlockDb for Database {
            fn block_height(&self) -> anyhow::Result<BlockHeight>;

            fn seal_block(
                &mut self,
                block_id: BlockId,
                consensus: FuelBlockConsensus,
            ) -> anyhow::Result<()>;
        }
    }

    fn make_tx(rng: &mut StdRng) -> Transaction {
        TransactionBuilder::create(rng.gen(), rng.gen(), vec![])
            .gas_price(rng.gen())
            .gas_limit(rng.gen())
            .finalize_without_signature_as_transaction()
    }

    #[tokio::test]
    async fn remove_skipped_transactions() {
        // The test verifies that if `BlockProducer` returns skipped transactions, they would
        // be propagated to `TxPool` for removal.
        let mut rng = StdRng::seed_from_u64(2322);
        let secret_key = SecretKey::random(&mut rng);

        let (_, stop) = mpsc::channel(1);
        let (_, txpool_broadcast) = broadcast::channel(1);
        let (import_block_events_tx, mut import_block_receiver_tx) =
            broadcast::channel(1);
        tokio::spawn(async move {
            import_block_receiver_tx.recv().await.unwrap();
        });

        const TX_NUM: usize = 100;
        let skipped_transactions: Vec<_> =
            (0..TX_NUM).map(|_| make_tx(&mut rng)).collect();

        let block_producer = MockBlockProducer {
            skipped_transactions: skipped_transactions.clone(),
        };

        let mut db = MockDatabase::default();
        db.expect_block_height()
            .returning(|| Ok(BlockHeight::from(1u32)));
        db.expect_seal_block().returning(|_, _| Ok(()));

        let mut txpool = MockTxPool::default();
        // Test created for only for this check.
        txpool.expect_remove_txs().returning(move |skipped_ids| {
            // Transform transactions into ids.
            let skipped_transactions: Vec<_> =
                skipped_transactions.iter().map(|tx| tx.id()).collect();

            // Check that all transactions are unique.
            let expected_skipped_ids_set: HashSet<_> =
                skipped_transactions.clone().into_iter().collect();
            assert_eq!(expected_skipped_ids_set.len(), TX_NUM);

            // Check that `TxPool::remove_txs` was called with the same ids in the same order.
            assert_eq!(skipped_ids.len(), TX_NUM);
            assert_eq!(skipped_transactions.len(), TX_NUM);
            assert_eq!(skipped_transactions, skipped_ids);
            Ok(vec![])
        });

        let mut task = Task {
            stop,
            block_gas_limit: 1000000,
            signing_key: Some(Secret::new(secret_key.into())),
            db,
            block_producer: Arc::new(block_producer),
            txpool,
            txpool_broadcast,
            import_block_events_tx,
            last_block_created: Instant::now(),
            trigger: Default::default(),
            timer: DeadlineClock::new(),
        };

        assert!(task.produce_block().await.is_ok());
    }
}
