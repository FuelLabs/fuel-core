use crate::{IndexExecutor, IndexerResult, Manifest, SchemaManager};
use fuel_gql_client::client::{FuelClient, PageDirection, PaginationRequest};
use fuel_tx::{ContractId, Receipt};
use fuels_core::abi_encoder::ABIEncoder;
use fuels_core::Tokenizable;
use std::collections::HashMap;
use std::net::SocketAddr;

use log::{debug, error, info, warn};

pub struct IndexerService {
    client: FuelClient,
    manager: SchemaManager,
    database_url: String,
    executors: HashMap<String, IndexExecutor>,
}

impl IndexerService {
    pub fn new(address: SocketAddr, database_url: String) -> IndexerResult<IndexerService> {
        let client = FuelClient::from(address);
        let manager = SchemaManager::new(&database_url)?;

        Ok(IndexerService {
            client,
            manager,
            database_url,
            executors: HashMap::default(),
        })
    }

    pub fn add_indexer(
        &mut self,
        manifest: Manifest,
        graphql_schema: &str,
        wasm_bytes: impl AsRef<[u8]>,
    ) -> IndexerResult<()> {
        let name = manifest.namespace.clone();
        let executor = IndexExecutor::new(self.database_url.clone(), manifest, wasm_bytes)?;

        let result = self.manager.new_schema(&name, graphql_schema)?;
        self.executors.insert(name.clone(), executor);
        info!("Registered indexer {}", name);
        Ok(())
    }

    fn get_registered_for(&self, id: ContractId) -> Option<Vec<&IndexExecutor>> {
        Some(vec![self.executors.get("demo_namespace").unwrap()])
    }

    pub async fn run(&self) {
        let blocks = self
            .client
            .blocks(PaginationRequest {
                cursor: None,
                results: 5,
                direction: PageDirection::Backward,
            })
            .await
            .unwrap();

        for block in blocks.results {
            for mut trans in block.transactions {
                let receipts = trans.receipts.take();
                let tx = fuel_tx::Transaction::try_from(trans).expect("Bad transaction");

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
                            Receipt::Log { id, ra, rb, .. } => {
                                if let Some(executors) = self.get_registered_for(id) {
                                    let token = fuels_core::Token::Struct(vec![
                                        id.into_token(),
                                        ra.into_token(),
                                        rb.into_token(),
                                    ]);
                                    for exe in executors {
                                        let args = ABIEncoder::new()
                                            .encode(&[token.clone()])
                                            .expect("FUCK");
                                        if let Err(e) = exe.trigger_event("an_event_name", args) {
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
    }
}
