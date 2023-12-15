use graph::{
    blockchain::ChainHeadUpdateStream,
    prelude::{
        futures03::{self, FutureExt},
        tokio, MetricsRegistry, StoreError,
    },
    prometheus::{CounterVec, GaugeVec},
    util::timed_rw_lock::TimedRwLock,
};
use std::collections::BTreeMap;
use std::sync::atomic;
use std::sync::{atomic::AtomicBool, Arc};

use lazy_static::lazy_static;

use crate::{
    connection_pool::ConnectionPool,
    notification_listener::{JsonNotification, NotificationListener, SafeChannelName},
    NotificationSender,
};
use graph::blockchain::ChainHeadUpdateListener as ChainHeadUpdateListenerTrait;
use graph::prelude::serde::{Deserialize, Serialize};
use graph::prelude::serde_json::{self, json};
use graph::prelude::tokio::sync::{mpsc::Receiver, watch};
use graph::prelude::{crit, debug, o, CheapClone, Logger, ENV_VARS};

lazy_static! {
    pub static ref CHANNEL_NAME: SafeChannelName =
        SafeChannelName::i_promise_this_is_safe("chain_head_updates");
}

struct Watcher {
    sender: Arc<watch::Sender<()>>,
    receiver: watch::Receiver<()>,
}

impl Watcher {
    fn new() -> Self {
        let (sender, receiver) = watch::channel(());
        Watcher {
            sender: Arc::new(sender),
            receiver,
        }
    }

    #[allow(dead_code)]
    fn send(&self) {
        // Unwrap: `self` holds a receiver.
        self.sender.send(()).unwrap()
    }
}

pub struct BlockIngestorMetrics {
    chain_head_number: Box<GaugeVec>,
}

impl BlockIngestorMetrics {
    pub fn new(registry: Arc<MetricsRegistry>) -> Self {
        Self {
            chain_head_number: registry
                .new_gauge_vec(
                    "ethereum_chain_head_number",
                    "Block number of the most recent block synced from Ethereum",
                    vec![String::from("network")],
                )
                .unwrap(),
        }
    }

    pub fn set_chain_head_number(&self, network_name: &str, chain_head_number: i64) {
        self.chain_head_number
            .with_label_values(vec![network_name].as_slice())
            .set(chain_head_number as f64);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ChainHeadUpdate {
    pub network_name: String,
    pub head_block_hash: String,
    pub head_block_number: u64,
}

pub struct ChainHeadUpdateListener {
    /// Update watchers keyed by network.
    watchers: Arc<TimedRwLock<BTreeMap<String, Watcher>>>,
    _listener: NotificationListener,
}

/// Sender for messages that the `ChainHeadUpdateListener` on other nodes
/// will receive. The sender is specific to a particular chain.
pub(crate) struct ChainHeadUpdateSender {
    pool: ConnectionPool,
    chain_name: String,
    sender: Arc<NotificationSender>,
}

impl ChainHeadUpdateListener {
    pub fn new(logger: &Logger, registry: Arc<MetricsRegistry>, postgres_url: String) -> Self {
        let logger = logger.new(o!("component" => "ChainHeadUpdateListener"));
        let ingestor_metrics = Arc::new(BlockIngestorMetrics::new(registry.clone()));
        let counter = registry
            .global_counter_vec(
                "notification_queue_recvd",
                "Number of messages received through Postgres LISTEN",
                vec!["channel", "network"].as_slice(),
            )
            .unwrap();
        // Create a Postgres notification listener for chain head updates
        let (mut listener, receiver) =
            NotificationListener::new(&logger, postgres_url, CHANNEL_NAME.clone());
        let watchers = Arc::new(TimedRwLock::new(
            BTreeMap::new(),
            "chain_head_listener_watchers",
        ));

        Self::listen(
            logger,
            ingestor_metrics,
            &mut listener,
            receiver,
            watchers.cheap_clone(),
            counter,
        );

        ChainHeadUpdateListener {
            watchers,

            // We keep the listener around to tie its stream's lifetime to
            // that of the chain head update listener and prevent it from
            // terminating early
            _listener: listener,
        }
    }

    fn listen(
        logger: Logger,
        metrics: Arc<BlockIngestorMetrics>,
        listener: &mut NotificationListener,
        mut receiver: Receiver<JsonNotification>,
        watchers: Arc<TimedRwLock<BTreeMap<String, Watcher>>>,
        counter: CounterVec,
    ) {
        // Process chain head updates in a dedicated task
        graph::spawn(async move {
            let sending_to_watcher = Arc::new(AtomicBool::new(false));
            while let Some(notification) = receiver.recv().await {
                // Create ChainHeadUpdate from JSON
                let update: ChainHeadUpdate =
                    match serde_json::from_value::<ChainHeadUpdate>(notification.payload.clone()) {
                        Ok(update) => {
                            let labels = [CHANNEL_NAME.as_str(), &update.network_name];
                            counter.with_label_values(&labels).inc();
                            update
                        }
                        Err(e) => {
                            crit!(
                                logger,
                                "invalid chain head update received from database";
                                "payload" => format!("{:?}", notification.payload),
                                "error" => e.to_string()
                            );
                            continue;
                        }
                    };

                // Observe the latest chain head for each network to monitor block ingestion
                metrics
                    .set_chain_head_number(&update.network_name, update.head_block_number as i64);

                // If there are subscriptions for this network, notify them.
                if let Some(watcher) = watchers.read(&logger).get(&update.network_name) {
                    // Due to a tokio bug, we must assume that the watcher can deadlock, see
                    // https://github.com/tokio-rs/tokio/issues/4246.
                    if !sending_to_watcher.load(atomic::Ordering::SeqCst) {
                        let sending_to_watcher = sending_to_watcher.cheap_clone();
                        let sender = watcher.sender.cheap_clone();
                        tokio::task::spawn_blocking(move || {
                            sending_to_watcher.store(true, atomic::Ordering::SeqCst);
                            sender.send(()).unwrap();
                            sending_to_watcher.store(false, atomic::Ordering::SeqCst);
                        });
                    } else {
                        debug!(logger, "skipping chain head update, watcher is deadlocked"; "network" => &update.network_name);
                    }
                }
            }
        });

        // We're ready, start listening to chain head updates
        listener.start();
    }
}

impl ChainHeadUpdateListenerTrait for ChainHeadUpdateListener {
    fn subscribe(&self, network_name: String, logger: Logger) -> ChainHeadUpdateStream {
        debug!(logger, "subscribing to chain head updates");

        let update_receiver = {
            let existing = {
                let watchers = self.watchers.read(&logger);
                watchers.get(&network_name).map(|w| w.receiver.clone())
            };

            if let Some(watcher) = existing {
                // Common case, this is not the first subscription for this network.
                watcher
            } else {
                // This is the first subscription for this network, a lock is required.
                //
                // Race condition: Another task could have simoultaneously entered this branch and
                // inserted a writer, so we should check the entry again after acquiring the lock.
                self.watchers
                    .write(&logger)
                    .entry(network_name)
                    .or_insert_with(Watcher::new)
                    .receiver
                    .clone()
            }
        };

        Box::new(futures03::stream::unfold(
            update_receiver,
            move |mut update_receiver| {
                let logger = logger.clone();
                async move {
                    // To be robust against any problems with the listener for the DB channel, a
                    // timeout is set so that subscribers are guaranteed to get periodic updates.
                    match tokio::time::timeout(
                        ENV_VARS.store.chain_head_watcher_timeout,
                        update_receiver.changed(),
                    )
                    .await
                    {
                        // Received an update.
                        Ok(Ok(())) => (),

                        // The sender was dropped, this should never happen.
                        Ok(Err(_)) => crit!(logger, "chain head watcher terminated"),

                        Err(_) => debug!(
                            logger,
                            "no chain head update for {} seconds, polling for update",
                            ENV_VARS.store.chain_head_watcher_timeout.as_secs()
                        ),
                    };
                    Some(((), update_receiver))
                }
                .boxed()
            },
        ))
    }
}

impl ChainHeadUpdateSender {
    pub fn new(
        pool: ConnectionPool,
        network_name: String,
        sender: Arc<NotificationSender>,
    ) -> Self {
        Self {
            pool,
            chain_name: network_name,
            sender,
        }
    }

    pub fn send(&self, hash: &str, number: i64) -> Result<(), StoreError> {
        let msg = json! ({
            "network_name": &self.chain_name,
            "head_block_hash": hash,
            "head_block_number": number
        });

        let conn = self.pool.get()?;
        self.sender
            .notify(&conn, CHANNEL_NAME.as_str(), Some(&self.chain_name), &msg)
    }
}
