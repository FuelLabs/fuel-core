use crate::{IndexExecutor, IndexerResult, Manifest, SchemaManager};
use async_std::sync::Arc;
use fuel_gql_client::client::{FuelClient, PageDirection, PaginatedResult, PaginationRequest};
use fuel_tx::{Receipt, Transaction};
use fuels_core::abi_encoder::ABIEncoder;
use fuels_core::{Token, Tokenizable};
use futures::stream::{futures_unordered::FuturesUnordered, StreamExt};
use serde::Deserialize;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::{
    task::JoinHandle,
    time::{sleep, Duration},
};

use tracing::{debug, error, info, warn};

#[derive(Clone, Deserialize)]
pub struct IndexerConfig {
    pub fuel_node_addr: SocketAddr,
    pub database_url: String,
    pub listen_endpoint: SocketAddr,
}

pub struct IndexerService {
    fuel_node_addr: SocketAddr,
    manager: SchemaManager,
    database_url: String,
    handles: HashMap<String, JoinHandle<()>>,
    killers: HashMap<String, Arc<AtomicBool>>,
}

impl IndexerService {
    pub fn new(config: IndexerConfig) -> IndexerResult<IndexerService> {
        let IndexerConfig {
            fuel_node_addr,
            database_url,
            ..
        } = config;
        let manager = SchemaManager::new(&database_url)?;

        Ok(IndexerService {
            fuel_node_addr,
            manager,
            database_url,
            handles: HashMap::default(),
            killers: HashMap::default(),
        })
    }

    pub fn add_indexer(
        &mut self,
        manifest: Manifest,
        graphql_schema: &str,
        wasm_bytes: impl AsRef<[u8]>,
        run_once: bool,
    ) -> IndexerResult<()> {
        let name = manifest.namespace.clone();
        let start_block = manifest.start_block;
        let _ = self.manager.new_schema(&name, graphql_schema)?;
        let executor = IndexExecutor::new(self.database_url.clone(), manifest, wasm_bytes)?;

        let kill_switch = Arc::new(AtomicBool::new(run_once));
        let handle = tokio::spawn(self.make_task(kill_switch.clone(), executor, start_block));

        info!("Registered indexer {}", name);
        self.handles.insert(name.clone(), handle);
        self.killers.insert(name, kill_switch);
        Ok(())
    }

    pub fn stop_indexer(&mut self, executor_name: &str) {
        if let Some(killer) = self.killers.remove(executor_name) {
            killer.store(true, Ordering::SeqCst);
        } else {
            warn!("Stop Indexer: No indexer with the name {executor_name}");
        }
    }

    fn make_task(
        &self,
        kill_switch: Arc<AtomicBool>,
        executor: IndexExecutor,
        start_block: Option<u64>,
    ) -> impl Future<Output = ()> {
        let mut next_cursor = None;
        let mut next_block = start_block.unwrap_or(1);
        let client = FuelClient::from(self.fuel_node_addr);
        let executor = Arc::new(executor);

        async move {
            loop {
                debug!("Fetching paginated results from {:?}", next_cursor);
                // TODO: can we have a "start at height" option?
                let PaginatedResult { cursor, results } = client
                    .blocks(PaginationRequest {
                        cursor: next_cursor,
                        results: 10,
                        direction: PageDirection::Forward,
                    })
                    .await
                    .unwrap();

                debug!("Processing {} results", results.len());
                let exec = executor.clone();

                let result = tokio::task::spawn_blocking(move || {
                    for block in results.into_iter().rev() {
                        if block.height.0 != next_block {
                            continue;
                        }
                        next_block = block.height.0 + 1;
                        for mut trans in block.transactions {
                            let receipts = trans.receipts.take();
                            let _tx = Transaction::try_from(trans).expect("Bad transaction");

                            if let Some(receipts) = receipts {
                                for receipt in receipts {
                                    let receipt = match Receipt::try_from(receipt) {
                                        Ok(r) => r,
                                        Err(e) => {
                                            error!("Receipt unpacking failed {:?}", e);
                                            continue; // Continue? or abort??
                                        }
                                    };

                                    match receipt {
                                        Receipt::Log {
                                            id,
                                            ra,
                                            rb,
                                            rc,
                                            rd,
                                            pc,
                                            is,
                                        } => {
                                            // TODO: might be nice to have Receipt type impl Tokenizable.
                                            let token = Token::Struct(vec![
                                                id.into_token(),
                                                ra.into_token(),
                                                rb.into_token(),
                                                rc.into_token(),
                                                rd.into_token(),
                                                pc.into_token(),
                                                is.into_token(),
                                            ]);

                                            let args = ABIEncoder::new()
                                                .encode(&[token.clone()])
                                                .expect("Bad Encoding!");
                                            // TODO: should wrap this in a db transaction.
                                            if let Err(e) =
                                                exec.trigger_event("an_event_name", vec![args])
                                            {
                                                error!("Event processing failed {:?}", e);
                                            }
                                        }
                                        o => warn!("Unhandled receipt type: {:?}", o),
                                    }
                                }
                            }
                        }
                    }
                })
                .await;

                if let Err(e) = result {
                    error!("Indexer executor failed {e:?}");
                }

                next_cursor = cursor;
                if next_cursor.is_none() {
                    info!("No next page, sleeping");
                    sleep(Duration::from_secs(5)).await;
                };

                if kill_switch.load(Ordering::SeqCst) {
                    break;
                }
            }
        }
    }

    pub async fn run(self) {
        let IndexerService { handles, .. } = self;
        let mut futs = FuturesUnordered::from_iter(handles.into_values());
        while let Some(fut) = futs.next().await {
            info!("Retired a future {fut:?}");
        }
    }
}
