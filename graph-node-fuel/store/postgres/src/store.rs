use async_trait::async_trait;
use std::sync::Arc;

use graph::{
    components::{
        server::index_node::VersionInfo,
        store::{
            BlockPtrForNumber, BlockStore as BlockStoreTrait, QueryPermit, QueryStoreManager,
            StatusStore, Store as StoreTrait,
        },
    },
    constraint_violation,
    data::subgraph::status,
    prelude::{
        web3::types::Address, BlockNumber, BlockPtr, CheapClone, DeploymentHash, PartialBlockPtr,
        QueryExecutionError, StoreError,
    },
};

use crate::{block_store::BlockStore, query_store::QueryStore, SubgraphStore};

/// The overall store of the system, consisting of a [`SubgraphStore`] and a
/// [`BlockStore`], each of which multiplex across multiple database shards.
/// The `SubgraphStore` is responsible for storing all data and metadata related
/// to individual subgraphs, and the `BlockStore` does the same for data belonging
/// to the chains that are being processed.
///
/// This struct should only be used during configuration and setup of `graph-node`.
/// Code that needs to access the store should use the traits from
/// [`graph::components::store`] and only require the smallest traits that are
/// suitable for their purpose.
#[derive(Clone)]
pub struct Store {
    subgraph_store: Arc<SubgraphStore>,
    block_store: Arc<BlockStore>,
}

impl Store {
    pub fn new(subgraph_store: Arc<SubgraphStore>, block_store: Arc<BlockStore>) -> Self {
        Self {
            subgraph_store,
            block_store,
        }
    }

    pub fn subgraph_store(&self) -> Arc<SubgraphStore> {
        self.subgraph_store.cheap_clone()
    }

    pub fn block_store(&self) -> Arc<BlockStore> {
        self.block_store.cheap_clone()
    }
}

impl StoreTrait for Store {
    type BlockStore = BlockStore;
    type SubgraphStore = SubgraphStore;

    fn subgraph_store(&self) -> Arc<Self::SubgraphStore> {
        self.subgraph_store.cheap_clone()
    }

    fn block_store(&self) -> Arc<Self::BlockStore> {
        self.block_store.cheap_clone()
    }
}

#[async_trait]
impl QueryStoreManager for Store {
    async fn query_store(
        &self,
        target: graph::data::query::QueryTarget,
        for_subscription: bool,
    ) -> Result<
        Arc<dyn graph::prelude::QueryStore + Send + Sync>,
        graph::prelude::QueryExecutionError,
    > {
        let store = self.subgraph_store.cheap_clone();
        let api_version = target.get_version();
        let target = target.clone();
        let (store, site, replica) = graph::spawn_blocking_allow_panic(move || {
            store
                .replica_for_query(target.clone(), for_subscription)
                .map_err(|e| e.into())
        })
        .await
        .map_err(|e| QueryExecutionError::Panic(e.to_string()))
        .and_then(|x| x)?;

        let chain_store = self.block_store.chain_store(&site.network).ok_or_else(|| {
            constraint_violation!(
                "Subgraphs index a known network, but {} indexes `{}` which we do not know about. This is most likely a configuration error.",
                site.deployment,
                site.network
            )
        })?;

        Ok(Arc::new(QueryStore::new(
            store,
            chain_store,
            site,
            replica,
            Arc::new(api_version.clone()),
        )))
    }
}

#[async_trait]
impl StatusStore for Store {
    fn status(&self, filter: status::Filter) -> Result<Vec<status::Info>, StoreError> {
        let mut infos = self.subgraph_store.status(filter)?;
        let ptrs = self.block_store.chain_head_pointers()?;
        for info in &mut infos {
            for chain in &mut info.chains {
                chain.chain_head_block = ptrs.get(&chain.network).map(|ptr| ptr.clone().into());
            }
        }
        Ok(infos)
    }

    fn version_info(&self, version_id: &str) -> Result<VersionInfo, StoreError> {
        let mut info = self.subgraph_store.version_info(version_id)?;

        info.total_ethereum_blocks_count = self.block_store.chain_head_block(&info.network)?;

        Ok(info)
    }

    fn versions_for_subgraph_id(
        &self,
        subgraph_id: &str,
    ) -> Result<(Option<String>, Option<String>), StoreError> {
        self.subgraph_store.versions_for_subgraph_id(subgraph_id)
    }

    fn subgraphs_for_deployment_hash(
        &self,
        deployment_hash: &str,
    ) -> Result<Vec<(String, String)>, StoreError> {
        self.subgraph_store
            .subgraphs_for_deployment_hash(deployment_hash)
    }

    async fn get_proof_of_indexing(
        &self,
        subgraph_id: &DeploymentHash,
        indexer: &Option<Address>,
        block: BlockPtr,
    ) -> Result<Option<[u8; 32]>, StoreError> {
        self.subgraph_store
            .get_proof_of_indexing(subgraph_id, indexer, block)
            .await
    }

    async fn get_public_proof_of_indexing(
        &self,
        subgraph_id: &DeploymentHash,
        block_number: BlockNumber,
        fetch_block_ptr: &dyn BlockPtrForNumber,
    ) -> Result<Option<(PartialBlockPtr, [u8; 32])>, StoreError> {
        self.subgraph_store
            .get_public_proof_of_indexing(
                subgraph_id,
                block_number,
                self.block_store().clone(),
                fetch_block_ptr,
            )
            .await
    }

    async fn query_permit(&self) -> Result<QueryPermit, StoreError> {
        // Status queries go to the primary shard.
        Ok(self.block_store.query_permit_primary().await)
    }
}
