use std::collections::HashMap;
use std::fmt;
use std::fmt::Write;
use std::result::Result;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::prelude::{SecondsFormat, Utc};
use futures03::TryFutureExt;
use http::header::CONTENT_TYPE;
use prometheus::Counter;
use reqwest;
use reqwest::Client;
use serde::ser::Serializer as SerdeSerializer;
use serde::Serialize;
use serde_json::json;
use slog::*;
use slog_async;

use crate::util::futures::retry;

/// General configuration parameters for Elasticsearch logging.
#[derive(Clone, Debug)]
pub struct ElasticLoggingConfig {
    /// The Elasticsearch service to log to.
    pub endpoint: String,
    /// The Elasticsearch username.
    pub username: Option<String>,
    /// The Elasticsearch password (optional).
    pub password: Option<String>,
    /// A client to serve as a connection pool to the endpoint.
    pub client: Client,
}

/// Serializes an slog log level using a serde Serializer.
fn serialize_log_level<S>(level: &Level, serializer: S) -> Result<S::Ok, S::Error>
where
    S: SerdeSerializer,
{
    serializer.serialize_str(match level {
        Level::Critical => "critical",
        Level::Error => "error",
        Level::Warning => "warning",
        Level::Info => "info",
        Level::Debug => "debug",
        Level::Trace => "trace",
    })
}

// Log message meta data.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ElasticLogMeta {
    module: String,
    line: i64,
    column: i64,
}

// Log message to be written to Elasticsearch.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ElasticLog {
    id: String,
    #[serde(flatten)]
    custom_id: HashMap<String, String>,
    arguments: HashMap<String, String>,
    timestamp: String,
    text: String,
    #[serde(serialize_with = "serialize_log_level")]
    level: Level,
    meta: ElasticLogMeta,
}

struct HashMapKVSerializer {
    kvs: Vec<(String, String)>,
}

impl HashMapKVSerializer {
    fn new() -> Self {
        HashMapKVSerializer {
            kvs: Default::default(),
        }
    }

    fn finish(self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        self.kvs.into_iter().for_each(|(k, v)| {
            map.insert(k, v);
        });
        map
    }
}

impl Serializer for HashMapKVSerializer {
    fn emit_arguments(&mut self, key: Key, val: &fmt::Arguments) -> slog::Result {
        self.kvs.push((key.into(), format!("{}", val)));
        Ok(())
    }
}

/// A super-simple slog Serializer for concatenating key/value arguments.
struct SimpleKVSerializer {
    kvs: Vec<(String, String)>,
}

impl SimpleKVSerializer {
    /// Creates a new `SimpleKVSerializer`.
    fn new() -> Self {
        SimpleKVSerializer {
            kvs: Default::default(),
        }
    }

    /// Collects all key/value arguments into a single, comma-separated string.
    /// Returns the number of key/value pairs and the string itself.
    fn finish(self) -> (usize, String) {
        (
            self.kvs.len(),
            self.kvs
                .iter()
                .map(|(k, v)| format!("{}: {}", k, v))
                .collect::<Vec<_>>()
                .join(", "),
        )
    }
}

impl Serializer for SimpleKVSerializer {
    fn emit_arguments(&mut self, key: Key, val: &fmt::Arguments) -> slog::Result {
        self.kvs.push((key.into(), format!("{}", val)));
        Ok(())
    }
}

/// Configuration for `ElasticDrain`.
#[derive(Clone, Debug)]
pub struct ElasticDrainConfig {
    /// General Elasticsearch logging configuration.
    pub general: ElasticLoggingConfig,
    /// The Elasticsearch index to log to.
    pub index: String,
    /// The name of the custom object id that the drain is for.
    pub custom_id_key: String,
    /// The custom id for the object that the drain is for.
    pub custom_id_value: String,
    /// The batching interval.
    pub flush_interval: Duration,
    /// Maximum retries in case of error.
    pub max_retries: usize,
}

/// An slog `Drain` for logging to Elasticsearch.
///
/// Writes logs to Elasticsearch using the following format:
/// ```ignore
/// {
///   "_index": "subgraph-logs",
///   "_id": "Qmb31zcpzqga7ERaUTp83gVdYcuBasz4rXUHFufikFTJGU-2018-11-08T00:54:52.589258000Z",
///   "_source": {
///     "level": "debug",
///     "timestamp": "2018-11-08T00:54:52.589258000Z",
///     "subgraphId": "Qmb31zcpzqga7ERaUTp83gVdYcuBasz4rXUHFufikFTJGU",
///     "meta": {
///       "module": "graph_chain_ethereum::block_stream",
///       "line": 220,
///       "column": 9
///     },
///     "text": "Chain head pointer, number: 6661038, hash: 0xf089c457700a57798ced06bd3f18eef53bb8b46510bcefaf13615a8a26e4424a, component: BlockStream",
///     "id": "Qmb31zcpzqga7ERaUTp83gVdYcuBasz4rXUHFufikFTJGU-2018-11-08T00:54:52.589258000Z"
///   }
/// }
/// ```
pub struct ElasticDrain {
    config: ElasticDrainConfig,
    error_logger: Logger,
    logs_sent_counter: Counter,
    logs: Arc<Mutex<Vec<ElasticLog>>>,
}

impl ElasticDrain {
    /// Creates a new `ElasticDrain`.
    pub fn new(
        config: ElasticDrainConfig,
        error_logger: Logger,
        logs_sent_counter: Counter,
    ) -> Self {
        let drain = ElasticDrain {
            config,
            error_logger,
            logs_sent_counter,
            logs: Arc::new(Mutex::new(vec![])),
        };
        drain.periodically_flush_logs();
        drain
    }

    fn periodically_flush_logs(&self) {
        let flush_logger = self.error_logger.clone();
        let logs_sent_counter = self.logs_sent_counter.clone();
        let logs = self.logs.clone();
        let config = self.config.clone();
        let mut interval = tokio::time::interval(self.config.flush_interval);
        let max_retries = self.config.max_retries;

        crate::task_spawn::spawn(async move {
            loop {
                interval.tick().await;

                let logs = logs.clone();
                let config = config.clone();
                let logs_to_send = {
                    let mut logs = logs.lock().unwrap();
                    let logs_to_send = (*logs).clone();
                    // Clear the logs, so the next batch can be recorded
                    logs.clear();
                    logs_to_send
                };

                // Do nothing if there are no logs to flush
                if logs_to_send.is_empty() {
                    continue;
                }

                logs_sent_counter.inc_by(logs_to_send.len() as f64);

                // The Elasticsearch batch API takes requests with the following format:
                // ```ignore
                // action_and_meta_data\n
                // optional_source\n
                // action_and_meta_data\n
                // optional_source\n
                // ```
                // For more details, see:
                // https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-bulk.html
                //
                // We're assembly the request body in the same way below:
                let batch_body = logs_to_send.iter().fold(String::from(""), |mut out, log| {
                    // Try to serialize the log itself to a JSON string
                    match serde_json::to_string(log) {
                        Ok(log_line) => {
                            // Serialize the action line to a string
                            let action_line = json!({
                                "index": {
                                    "_index": config.index,
                                    "_id": log.id,
                                }
                            })
                            .to_string();

                            // Combine the two lines with newlines, make sure there is
                            // a newline at the end as well
                            out.push_str(format!("{}\n{}\n", action_line, log_line).as_str());
                        }
                        Err(e) => {
                            error!(
                                flush_logger,
                                "Failed to serialize Elasticsearch log to JSON: {}", e
                            );
                        }
                    };

                    out
                });

                // Build the batch API URL
                let mut batch_url = reqwest::Url::parse(config.general.endpoint.as_str())
                    .expect("invalid Elasticsearch URL");
                batch_url.set_path("_bulk");

                // Send batch of logs to Elasticsearch
                let header = match config.general.username {
                    Some(username) => config
                        .general
                        .client
                        .post(batch_url)
                        .header(CONTENT_TYPE, "application/json")
                        .basic_auth(username, config.general.password.clone()),
                    None => config
                        .general
                        .client
                        .post(batch_url)
                        .header(CONTENT_TYPE, "application/json"),
                };

                retry("send logs to elasticsearch", &flush_logger)
                    .limit(max_retries)
                    .timeout_secs(30)
                    .run(move || {
                        header
                            .try_clone()
                            .unwrap() // Unwrap: Request body not yet set
                            .body(batch_body.clone())
                            .send()
                            .and_then(|response| async { response.error_for_status() })
                            .map_ok(|_| ())
                    })
                    .await
                    .unwrap_or_else(|e| {
                        // Log if there was a problem sending the logs
                        error!(flush_logger, "Failed to send logs to Elasticsearch: {}", e);
                    })
            }
        });
    }
}

impl Drain for ElasticDrain {
    type Ok = ();
    type Err = ();

    fn log(&self, record: &Record, values: &OwnedKVList) -> Result<Self::Ok, Self::Err> {
        // Don't sent `trace` logs to ElasticSearch.
        if record.level() == Level::Trace {
            return Ok(());
        }
        let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true);
        let id = format!("{}-{}", self.config.custom_id_value, timestamp);

        // Serialize logger arguments
        let mut serializer = SimpleKVSerializer::new();
        record
            .kv()
            .serialize(record, &mut serializer)
            .expect("failed to serializer logger arguments");
        let (n_logger_kvs, logger_kvs) = serializer.finish();

        // Serialize log message arguments
        let mut serializer = SimpleKVSerializer::new();
        values
            .serialize(record, &mut serializer)
            .expect("failed to serialize log message arguments");
        let (n_value_kvs, value_kvs) = serializer.finish();

        // Serialize log message arguments into hash map
        let mut serializer = HashMapKVSerializer::new();
        record
            .kv()
            .serialize(record, &mut serializer)
            .expect("failed to serialize log message arguments into hash map");
        let arguments = serializer.finish();

        let mut text = format!("{}", record.msg());
        if n_logger_kvs > 0 {
            write!(text, ", {}", logger_kvs).unwrap();
        }
        if n_value_kvs > 0 {
            write!(text, ", {}", value_kvs).unwrap();
        }

        // Prepare custom id for log document
        let mut custom_id = HashMap::new();
        custom_id.insert(
            self.config.custom_id_key.clone(),
            self.config.custom_id_value.clone(),
        );

        // Prepare log document
        let log = ElasticLog {
            id,
            custom_id,
            arguments,
            timestamp,
            text,
            level: record.level(),
            meta: ElasticLogMeta {
                module: record.module().into(),
                line: record.line() as i64,
                column: record.column() as i64,
            },
        };

        // Push the log into the queue
        let mut logs = self.logs.lock().unwrap();
        logs.push(log);

        Ok(())
    }
}

/// Creates a new asynchronous Elasticsearch logger.
///
/// Uses `error_logger` to print any Elasticsearch logging errors,
/// so they don't go unnoticed.
pub fn elastic_logger(
    config: ElasticDrainConfig,
    error_logger: Logger,
    logs_sent_counter: Counter,
) -> Logger {
    let elastic_drain = ElasticDrain::new(config, error_logger, logs_sent_counter).fuse();
    let async_drain = slog_async::Async::new(elastic_drain)
        .chan_size(20000)
        .build()
        .fuse();
    Logger::root(async_drain, o!())
}
