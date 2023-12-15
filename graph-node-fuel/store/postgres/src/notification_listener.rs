use diesel::pg::PgConnection;
use diesel::select;
use diesel::sql_types::Text;
use graph::prelude::tokio::sync::mpsc::error::SendTimeoutError;
use graph::util::backoff::ExponentialBackoff;
use lazy_static::lazy_static;
use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
use postgres::Notification;
use postgres::{fallible_iterator::FallibleIterator, Client};
use postgres_openssl::MakeTlsConnector;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{channel, Receiver};

use graph::prelude::serde_json;
use graph::prelude::*;

#[cfg(debug_assertions)]
lazy_static::lazy_static! {
    /// Tests set this to true so that `send_store_event` will store a copy
    /// of each event sent in `EVENT_TAP`
    pub static ref EVENT_TAP_ENABLED: Mutex<bool> = Mutex::new(false);
    pub static ref EVENT_TAP: Mutex<Vec<StoreEvent>> = Mutex::new(Vec::new());
}

#[derive(Clone)]
/// This newtype exists to make it hard to misuse the `NotificationListener` API in a way that
/// could impact security.
pub struct SafeChannelName(String);

impl SafeChannelName {
    /// Channel names must be valid Postgres SQL identifiers.
    /// If a channel name is provided that is not a valid identifier,
    /// then there is a security risk due to SQL injection.
    /// Unfortunately, it is difficult to properly validate a channel name.
    /// (A blacklist would have to include SQL keywords, for example)
    ///
    /// The caller of this method is promising that the supplied channel name
    /// is a valid Postgres identifier and cannot be supplied (even partially)
    /// by an attacker.
    pub fn i_promise_this_is_safe(channel_name: impl Into<String>) -> Self {
        SafeChannelName(channel_name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub struct NotificationListener {
    worker_handle: Option<thread::JoinHandle<()>>,
    terminate_worker: Arc<AtomicBool>,
    worker_barrier: Arc<Barrier>,
    started: bool,
}

impl NotificationListener {
    /// Connect to the specified database and listen for Postgres notifications on the specified
    /// channel.
    ///
    /// Must call `.start()` to begin receiving notifications on the returned receiver.
    //
    /// The listener will handle dropping the database connection by
    /// indefinitely trying to reconnect to the database. Users of the
    /// listener have no way to find out whether the connection had been
    /// dropped and was reestablished.
    pub fn new(
        logger: &Logger,
        postgres_url: String,
        channel_name: SafeChannelName,
    ) -> (Self, Receiver<JsonNotification>) {
        // Listen to Postgres notifications in a worker thread
        let (receiver, worker_handle, terminate_worker, worker_barrier) =
            Self::listen(logger, postgres_url, channel_name);

        (
            NotificationListener {
                worker_handle: Some(worker_handle),
                terminate_worker,
                worker_barrier,
                started: false,
            },
            receiver,
        )
    }

    /// Start accepting notifications.
    /// Must be called for any notifications to be received.
    pub fn start(&mut self) {
        if !self.started {
            self.worker_barrier.wait();
            self.started = true;
        }
    }

    fn listen(
        logger: &Logger,
        postgres_url: String,
        channel_name: SafeChannelName,
    ) -> (
        Receiver<JsonNotification>,
        thread::JoinHandle<()>,
        Arc<AtomicBool>,
        Arc<Barrier>,
    ) {
        /// Connect to the database at `postgres_url` and do a `LISTEN
        /// {channel_name}`. If that fails, retry with exponential backoff
        /// with a delay between 1s and 32s
        ///
        /// If `barrier` is given, call `barrier.wait()` after the first
        /// attempt to connect. When the database is up, we make sure that
        /// we listen before other work that depends on receiving all
        /// notifications progresses, and when the database is down, that we
        /// do not unduly block everything.
        fn connect_and_listen(
            logger: &Logger,
            postgres_url: &str,
            channel_name: &str,
            barrier: Option<&Barrier>,
        ) -> Client {
            let mut backoff =
                ExponentialBackoff::new(Duration::from_secs(1), Duration::from_secs(30));
            loop {
                let mut builder = SslConnector::builder(SslMethod::tls())
                    .expect("unable to create SslConnector builder");
                builder.set_verify(SslVerifyMode::NONE);
                let connector = MakeTlsConnector::new(builder.build());

                let res = Client::connect(postgres_url, connector).and_then(|mut conn| {
                    conn.execute(format!("LISTEN {}", channel_name).as_str(), &[])?;
                    Ok(conn)
                });
                barrier.map(|barrier| barrier.wait());
                match res {
                    Err(e) => {
                        error!(logger, "Failed to connect notification listener: {}", e;
                                       "attempt" => backoff.attempt,
                                       "retry_delay_s" => backoff.delay().as_secs());
                        backoff.sleep();
                    }
                    Ok(conn) => {
                        return conn;
                    }
                }
            }
        }

        let logger = logger.new(o!(
            "component" => "NotificationListener",
            "channel" => channel_name.0.clone()
        ));

        debug!(
            logger,
            "Cleaning up large notifications after about {}s",
            ENV_VARS.store.large_notification_cleanup_interval.as_secs(),
        );

        // Create two ends of a boolean variable for signalling when the worker
        // thread should be terminated
        let terminate = Arc::new(AtomicBool::new(false));
        let terminate_worker = terminate.clone();
        let barrier = Arc::new(Barrier::new(2));
        let worker_barrier = barrier.clone();

        // Create a channel for notifications
        let (sender, receiver) = channel(100);

        let worker_handle = graph::spawn_thread("notification_listener", move || {
            // We exit the process on panic so unwind safety is irrelevant.
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                let mut connected = true;
                let mut conn = connect_and_listen(
                    &logger,
                    postgres_url.as_str(),
                    &channel_name.0,
                    Some(barrier.as_ref()),
                );

                let mut max_queue_size_seen = 0;

                // Read notifications until the thread is to be terminated
                while !terminate.load(Ordering::SeqCst) {
                    if !connected {
                        conn = connect_and_listen(
                            &logger,
                            postgres_url.as_str(),
                            &channel_name.0,
                            None,
                        );
                        debug!(logger, "Reconnected notification listener");
                        connected = true;
                    }

                    let queue_size = conn.notifications().len();
                    if queue_size > 1000 && queue_size > max_queue_size_seen {
                        debug!(logger, "Large notification queue to process";
                                    "queue_size" => queue_size,
                                    "channel" => &channel_name.0,
                        );
                    }
                    max_queue_size_seen = queue_size.max(max_queue_size_seen);

                    // Obtain pending notifications from Postgres. We load
                    // them all into memory, since for large notifications
                    // we need to query the database again; avoiding this
                    // load would require that we use a second database
                    // connection to look up large notifications
                    //
                    // We batch notifications such that we do not wait for
                    // longer than 500ms for new notifications to arrive,
                    // but limit the size of each batch to 128 to guarantee
                    // progress on a busy system
                    let notifications: Vec<_> = conn
                        .notifications()
                        .timeout_iter(Duration::from_millis(500))
                        .iterator()
                        .take(128)
                        .filter_map(|item| match item {
                            Ok(msg) => Some(msg),
                            Err(e) => {
                                // When there's an error, process whatever
                                // notifications we've picked up so far, and
                                // cause the start of the loop to reconnect
                                if connected {
                                    let msg = format!("{}", e);
                                    crit!(logger, "Error receiving message"; "error" => &msg);
                                }
                                connected = false;
                                None
                            }
                        })
                        .filter(|notification| notification.channel() == channel_name.0)
                        .collect();

                    // Read notifications until there hasn't been one for 500ms
                    for notification in notifications {
                        // Terminate the thread if desired
                        if terminate.load(Ordering::SeqCst) {
                            break;
                        }

                        match JsonNotification::parse(&notification, &mut conn) {
                            Ok(json_notification) => {
                                let timeout = ENV_VARS.store.notification_broadcast_timeout;
                                match graph::block_on(
                                    sender.send_timeout(json_notification, timeout),
                                ) {
                                    // on error or timeout, continue
                                    Ok(()) => (),
                                    Err(SendTimeoutError::Timeout(_)) => {
                                        crit!(
                                            logger,
                                            "Timeout broadcasting DB notification, skipping it";
                                            "timeout_secs" => timeout.as_secs().to_string(),
                                            "channel" => &channel_name.0,
                                            "notification" => format!("{:?}", notification),
                                        );
                                    }

                                    // If sending fails, this means that the receiver has been
                                    // dropped and we should terminate the listener loop.
                                    Err(SendTimeoutError::Closed(_)) => {
                                        debug!(
                                            logger,
                                            "DB notification listener closed";
                                            "channel" => &channel_name.0,
                                        );
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                crit!(
                                    logger,
                                    "Failed to parse database notification";
                                    "notification" => format!("{:?}", notification),
                                    "error" => format!("{}", e),
                                );
                                continue;
                            }
                        }
                    }
                }
            }))
            .unwrap_or_else(|_| std::process::exit(1))
        });

        (receiver, worker_handle, terminate_worker, worker_barrier)
    }
}

impl Drop for NotificationListener {
    fn drop(&mut self) {
        // When dropping the listener, also make sure we signal termination
        // to the worker and wait for it to shut down
        self.terminate_worker.store(true, Ordering::SeqCst);
        self.worker_handle
            .take()
            .unwrap()
            .join()
            .expect("failed to terminate NotificationListener thread");
    }
}

mod public {
    table! {
        large_notifications(id) {
            id -> Integer,
            payload -> Text,
            created_at -> Timestamp,
        }
    }
}

// A utility to send JSON notifications that may be larger than the
// 8000 bytes limit for Postgres NOTIFY payloads. Large notifications
// are written to the `large_notifications` table and their ID is sent
// via NOTIFY in place of the actual payload. Consumers of large
// notifications are then responsible to fetch the actual payload from
// the `large_notifications` table.
#[derive(Debug)]
pub struct JsonNotification {
    pub process_id: i32,
    pub channel: String,
    pub payload: serde_json::Value,
}

// Any payload bigger than this is considered large. Any notification larger
// than this will be put into the `large_notifications` table, and only
// its id in the table will be sent via `notify`
static LARGE_NOTIFICATION_THRESHOLD: usize = 7800;

impl JsonNotification {
    pub fn parse(
        notification: &Notification,
        conn: &mut Client,
    ) -> Result<JsonNotification, StoreError> {
        let value = serde_json::from_str(notification.payload())?;

        match value {
            serde_json::Value::Number(n) => {
                let payload_id: i64 = n.as_i64().ok_or_else(|| {
                    anyhow!("Invalid notification ID, not compatible with i64: {}", n)
                })?;

                if payload_id < (i32::min_value() as i64) || payload_id > (i32::max_value() as i64)
                {
                    Err(anyhow!(
                        "Invalid notification ID, value exceeds i32: {}",
                        payload_id
                    ))?;
                }

                let payload_rows = conn
                    .query(
                        "SELECT payload FROM large_notifications WHERE id = $1",
                        &[&(payload_id as i32)],
                    )
                    .map_err(|e| {
                        anyhow!(
                            "Error retrieving payload for notification {}: {}",
                            payload_id,
                            e
                        )
                    })?;

                if payload_rows.is_empty() || payload_rows.get(0).is_none() {
                    return Err(anyhow!("No payload found for notification {}", payload_id))?;
                }
                let payload: String = payload_rows.get(0).unwrap().get(0);

                Ok(JsonNotification {
                    process_id: notification.process_id(),
                    channel: notification.channel().to_string(),
                    payload: serde_json::from_str(&payload)?,
                })
            }
            serde_json::Value::Object(_) => Ok(JsonNotification {
                process_id: notification.process_id(),
                channel: notification.channel().to_string(),
                payload: value,
            }),
            _ => Err(anyhow!("JSON notifications must be numbers or objects"))?,
        }
    }
}

/// Send notifications via `pg_notify`. All sending of notifications through
/// Postgres should go through this struct as it maintains a Prometheus
/// metric to track the amount of messages sent
pub struct NotificationSender {
    sent_counter: CounterVec,
}

impl NotificationSender {
    pub fn new(registry: Arc<MetricsRegistry>) -> Self {
        let sent_counter = registry
            .global_counter_vec(
                "notification_queue_sent",
                "Number of messages sent through pg_notify()",
                &["channel", "network"],
            )
            .expect("we can create the notification_queue_sent gauge");
        NotificationSender { sent_counter }
    }

    /// Send `data` as a Postgres notification on the given `channel`. The
    /// connection `conn` must be into the primary database as that's the
    /// only place where listeners connect. The `network` is only used for
    /// metrics gathering and does not affect how the notification is sent
    pub fn notify(
        &self,
        conn: &PgConnection,
        channel: &str,
        network: Option<&str>,
        data: &serde_json::Value,
    ) -> Result<(), StoreError> {
        use diesel::ExpressionMethods;
        use diesel::RunQueryDsl;
        use public::large_notifications::dsl::*;

        sql_function! {
            fn pg_notify(channel: Text, msg: Text)
        }

        let msg = data.to_string();

        if msg.len() <= LARGE_NOTIFICATION_THRESHOLD {
            select(pg_notify(channel, &msg)).execute(conn)?;
        } else {
            // Write the notification payload to the large_notifications table
            let payload_id: i32 = diesel::insert_into(large_notifications)
                .values(payload.eq(&msg))
                .returning(id)
                .get_result(conn)?;

            // Use the large_notifications row ID as the payload for NOTIFY
            select(pg_notify(channel, &payload_id.to_string())).execute(conn)?;

            // Prune old large_notifications. We want to keep the size of the
            // table manageable, but there's a lot of latitude in how often
            // we need to clean up before old notifications slow down
            // data access.
            //
            // To avoid checking whether cleanup is needed too often, we
            // only check once every LARGE_NOTIFICATION_CLEANUP_INTERVAL
            // (per graph-node process) It would be even better to do this
            // cleanup only once for all graph-node processes accessing the
            // same database, but that requires a lot more infrastructure
            lazy_static! {
                static ref LAST_CLEANUP_CHECK: Mutex<Instant> = Mutex::new(Instant::now());
            }

            // If we can't get the lock, another thread in this process is
            // already checking, and we can just skip checking
            if let Ok(mut last_check) = LAST_CLEANUP_CHECK.try_lock() {
                if last_check.elapsed() > ENV_VARS.store.large_notification_cleanup_interval {
                    diesel::sql_query(format!(
                        "delete from large_notifications
                         where created_at < current_timestamp - interval '{}s'",
                        ENV_VARS.store.large_notification_cleanup_interval.as_secs(),
                    ))
                    .execute(conn)?;
                    *last_check = Instant::now();
                }
            }
        }
        self.sent_counter
            .with_label_values(&[channel, network.unwrap_or("none")])
            .inc();
        Ok(())
    }
}
