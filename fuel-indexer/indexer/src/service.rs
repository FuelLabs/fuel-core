use crate::{IndexExecutor, IndexerResult, Manifest, SchemaManager};
use fuel_gql_client::client::{FuelClient, PageDirection, PaginatedResult, PaginationRequest};
use fuel_tx::{ContractId, Receipt, Transaction};
use fuels_core::abi_encoder::ABIEncoder;
use fuels_core::{Token, Tokenizable};
use serde::Deserialize;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::net::SocketAddr;

use log::{debug, error, info, warn};

#[derive(Clone, Deserialize)]
pub struct IndexerConfig {
    pub fuel_node_addr: SocketAddr,
    pub database_url: String,
    pub listen_endpoint: SocketAddr,
}

pub struct IndexerService {
    client: FuelClient,
    manager: SchemaManager,
    database_url: String,
    executors: HashMap<String, IndexExecutor>,
}

impl IndexerService {
    pub fn new(config: IndexerConfig) -> IndexerResult<IndexerService> {
        let client = FuelClient::from(config.fuel_node_addr);
        let manager = SchemaManager::new(&config.database_url)?;

        Ok(IndexerService {
            client,
            manager,
            database_url: config.database_url,
            executors: HashMap::default(),
        })
    }

    // TODO: wasm executors should spawn to a blocking thread....
    pub fn add_indexer(
        &mut self,
        manifest: Manifest,
        graphql_schema: &str,
        wasm_bytes: impl AsRef<[u8]>,
    ) -> IndexerResult<()> {
        let name = manifest.namespace.clone();
        let _ = self.manager.new_schema(&name, graphql_schema)?;
        let executor = IndexExecutor::new(self.database_url.clone(), manifest, wasm_bytes)?;

        self.executors.insert(name.clone(), executor);
        info!("Registered indexer {}", name);
        Ok(())
    }

    fn get_registered_for(&self, _: ContractId) -> Option<Vec<&IndexExecutor>> {
        Some(vec![self.executors.get("demo_namespace").unwrap()])
    }

    pub async fn run(self, run_once: bool) {
        let mut next_cursor = None;
        let mut next_block = 0;

        loop {
            debug!("Fetching paginated results from {:?}", next_cursor);
            let PaginatedResult { cursor, results } = self
                .client
                .blocks(PaginationRequest {
                    cursor: next_cursor,
                    results: 5,
                    direction: PageDirection::Backward,
                })
                .await
                .unwrap();

            debug!("Processing {} results", results.len());
            for block in results {
                if block.height.0 < next_block {
                    // TODO: sleep?
                    continue;
                }
                next_block += 1;
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
                                    if let Some(executors) = self.get_registered_for(id) {
                                        // TODO: might be nice to have a derive macro for Tokenizable...
                                        let token = Token::Struct(vec![
                                            id.into_token(),
                                            ra.into_token(),
                                            rb.into_token(),
                                            rc.into_token(),
                                            rd.into_token(),
                                            pc.into_token(),
                                            is.into_token(),
                                        ]);
                                        for exe in executors {
                                            let args = ABIEncoder::new()
                                                .encode(&[token.clone()])
                                                .expect("FUCK");
                                            if let Err(e) =
                                                exe.trigger_event("an_event_name", vec![args])
                                            {
                                                error!("Event processing failed {:?}", e);
                                            }
                                        }
                                    }
                                }
                                o => warn!("Unhandled receipt type: {:?}", o),
                            }
                        }
                    }
                }
            }
            next_cursor = cursor;

            if run_once {
                break;
            }
        }
    }
}
