use futures03::TryStreamExt;
use graph::parking_lot::Mutex;
use graph::tokio_stream::wrappers::ReceiverStream;
use std::collections::BTreeSet;
use std::sync::{atomic::Ordering, Arc, RwLock};
use std::{collections::HashMap, sync::atomic::AtomicUsize};
use tokio::sync::mpsc::{channel, Sender};
use tokio::sync::watch;
use uuid::Uuid;

use crate::notification_listener::{NotificationListener, SafeChannelName};
use graph::components::store::{SubscriptionManager as SubscriptionManagerTrait, UnitStream};
use graph::prelude::serde_json;
use graph::{prelude::*, tokio_stream};

pub struct StoreEventListener {
    notification_listener: NotificationListener,
}

impl StoreEventListener {
    pub fn new(
        logger: Logger,
        postgres_url: String,
        registry: Arc<MetricsRegistry>,
    ) -> (Self, Box<dyn Stream<Item = StoreEvent, Error = ()> + Send>) {
        let channel = SafeChannelName::i_promise_this_is_safe("store_events");
        let (notification_listener, receiver) =
            NotificationListener::new(&logger, postgres_url, channel.clone());

        let counter = registry
            .global_counter_vec(
                "notification_queue_recvd",
                "Number of messages received through Postgres LISTEN",
                vec!["channel", "network"].as_slice(),
            )
            .unwrap()
            .with_label_values(&[channel.as_str(), "none"]);

        let event_stream = Box::new(
            ReceiverStream::new(receiver)
                .map(Result::<_, ()>::Ok)
                .compat()
                .filter_map(move |notification| {
                    // When graph-node is starting up, it is possible that
                    // Postgres still has old messages queued up that we
                    // can't decode anymore. It is safe to skip them; once
                    // We've seen 10 valid messages, we can assume that
                    // whatever old messages Postgres had queued have been
                    // cleared. Seeing an invalid message after that
                    // definitely indicates trouble.
                    let num_valid = AtomicUsize::new(0);
                    serde_json::from_value(notification.payload.clone()).map_or_else(
                        |_err| {
                            error!(
                                &logger,
                                "invalid store event received from database: {:?}",
                                notification.payload
                            );
                            if num_valid.load(Ordering::SeqCst) > 10 {
                                panic!(
                                    "invalid store event received from database: {:?}",
                                    notification.payload
                                );
                            }
                            None
                        },
                        |change| {
                            num_valid.fetch_add(1, Ordering::SeqCst);
                            counter.inc();
                            Some(change)
                        },
                    )
                }),
        );

        (
            StoreEventListener {
                notification_listener,
            },
            event_stream,
        )
    }

    pub fn start(&mut self) {
        self.notification_listener.start()
    }
}

struct Watcher<T> {
    sender: Arc<watch::Sender<T>>,
    receiver: watch::Receiver<T>,
}

impl<T: Clone + Debug + Send + Sync + 'static> Watcher<T> {
    fn new(init: T) -> Self {
        let (sender, receiver) = watch::channel(init);
        Watcher {
            sender: Arc::new(sender),
            receiver,
        }
    }

    fn send(&self, v: T) {
        // Unwrap: `self` holds a receiver.
        self.sender.send(v).unwrap()
    }

    fn stream(&self) -> Box<dyn futures03::Stream<Item = T> + Unpin + Send + Sync> {
        Box::new(tokio_stream::wrappers::WatchStream::new(
            self.receiver.clone(),
        ))
    }

    /// Outstanding receivers returned from `Self::stream`.
    fn receiver_count(&self) -> usize {
        // Do not count the internal receiver.
        self.sender.receiver_count() - 1
    }
}

/// Manage subscriptions to the `StoreEvent` stream. Keep a list of
/// currently active subscribers and forward new events to each of them
pub struct SubscriptionManager {
    // These are more efficient since only one entry is stored per filter.
    subscriptions_no_payload: Arc<Mutex<HashMap<BTreeSet<SubscriptionFilter>, Watcher<()>>>>,

    subscriptions:
        Arc<RwLock<HashMap<String, (Arc<BTreeSet<SubscriptionFilter>>, Sender<Arc<StoreEvent>>)>>>,

    /// Keep the notification listener alive
    listener: StoreEventListener,
}

impl SubscriptionManager {
    pub fn new(logger: Logger, postgres_url: String, registry: Arc<MetricsRegistry>) -> Self {
        let (listener, store_events) = StoreEventListener::new(logger, postgres_url, registry);

        let mut manager = SubscriptionManager {
            subscriptions_no_payload: Arc::new(Mutex::new(HashMap::new())),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            listener,
        };

        // Deal with store subscriptions
        manager.handle_store_events(store_events);
        manager.periodically_clean_up_stale_subscriptions();

        manager.listener.start();

        manager
    }

    /// Receive store events from Postgres and send them to all active
    /// subscriptions. Detect stale subscriptions in the process and
    /// close them.
    fn handle_store_events(
        &self,
        store_events: Box<dyn Stream<Item = StoreEvent, Error = ()> + Send>,
    ) {
        let subscriptions = self.subscriptions.cheap_clone();
        let subscriptions_no_payload = self.subscriptions_no_payload.cheap_clone();
        let mut store_events = store_events.compat();

        // This channel is constantly receiving things and there are locks involved,
        // so it's best to use a blocking task.
        graph::spawn_blocking(async move {
            while let Some(Ok(event)) = store_events.next().await {
                let event = Arc::new(event);

                // Send to `subscriptions`.
                {
                    let senders = subscriptions.read().unwrap().clone();

                    // Write change to all matching subscription streams; remove subscriptions
                    // whose receiving end has been dropped
                    for (id, (_, sender)) in senders
                        .iter()
                        .filter(|(_, (filter, _))| event.matches(filter))
                    {
                        if sender.send(event.cheap_clone()).await.is_err() {
                            // Receiver was dropped
                            subscriptions.write().unwrap().remove(id);
                        }
                    }
                }

                // Send to `subscriptions_no_payload`.
                {
                    let watchers = subscriptions_no_payload.lock();

                    // Write change to all matching subscription streams
                    for (_, watcher) in watchers.iter().filter(|(filter, _)| event.matches(filter))
                    {
                        watcher.send(());
                    }
                }
            }
        });
    }

    fn periodically_clean_up_stale_subscriptions(&self) {
        let subscriptions = self.subscriptions.cheap_clone();
        let subscriptions_no_payload = self.subscriptions_no_payload.cheap_clone();

        // Clean up stale subscriptions every 5s
        graph::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            loop {
                interval.tick().await;

                // Cleanup `subscriptions`.
                {
                    let mut subscriptions = subscriptions.write().unwrap();

                    // Obtain IDs of subscriptions whose receiving end has gone
                    let stale_ids = subscriptions
                        .iter_mut()
                        .filter_map(|(id, (_, sender))| match sender.is_closed() {
                            true => Some(id.clone()),
                            false => None,
                        })
                        .collect::<Vec<_>>();

                    // Remove all stale subscriptions
                    for id in stale_ids {
                        subscriptions.remove(&id);
                    }
                }

                // Cleanup `subscriptions_no_payload`.
                {
                    let mut subscriptions = subscriptions_no_payload.lock();

                    // Obtain IDs of subscriptions whose receiving end has gone
                    let stale_ids = subscriptions
                        .iter_mut()
                        .filter_map(|(id, watcher)| match watcher.receiver_count() == 0 {
                            true => Some(id.clone()),
                            false => None,
                        })
                        .collect::<Vec<_>>();

                    // Remove all stale subscriptions
                    for id in stale_ids {
                        subscriptions.remove(&id);
                    }
                }
            }
        });
    }
}

impl SubscriptionManagerTrait for SubscriptionManager {
    fn subscribe(&self, entities: BTreeSet<SubscriptionFilter>) -> StoreEventStreamBox {
        let id = Uuid::new_v4().to_string();

        // Prepare the new subscription by creating a channel and a subscription object
        let (sender, receiver) = channel(100);

        // Add the new subscription
        self.subscriptions
            .write()
            .unwrap()
            .insert(id, (Arc::new(entities.clone()), sender));

        // Return the subscription ID and entity change stream
        StoreEventStream::new(Box::new(ReceiverStream::new(receiver).map(Ok).compat()))
            .filter_by_entities(entities)
    }

    fn subscribe_no_payload(&self, entities: BTreeSet<SubscriptionFilter>) -> UnitStream {
        self.subscriptions_no_payload
            .lock()
            .entry(entities)
            .or_insert_with(|| Watcher::new(()))
            .stream()
    }
}
