use super::scalars::{
    U32,
    U64,
};
use crate::fuel_core_graphql_api::{
    Config as GraphQLConfig,
    QUERY_COSTS,
};
use async_graphql::{
    Context,
    Object,
};
use std::time::UNIX_EPOCH;

pub struct NodeInfo {
    utxo_validation: bool,
    vm_backtrace: bool,
    max_tx: U64,
    max_depth: U64,
    node_version: String,
}

#[Object]
impl NodeInfo {
    async fn utxo_validation(&self) -> bool {
        self.utxo_validation
    }

    async fn vm_backtrace(&self) -> bool {
        self.vm_backtrace
    }

    async fn max_tx(&self) -> U64 {
        self.max_tx
    }

    async fn max_depth(&self) -> U64 {
        self.max_depth
    }

    async fn node_version(&self) -> String {
        self.node_version.to_owned()
    }

    #[graphql(complexity = "QUERY_COSTS.get_peers + child_complexity")]
    async fn peers(&self, _ctx: &Context<'_>) -> async_graphql::Result<Vec<PeerInfo>> {
        #[cfg(feature = "p2p")]
        {
            let p2p: &crate::fuel_core_graphql_api::api_service::P2pService =
                _ctx.data_unchecked();
            let peer_info = p2p.all_peer_info().await?;
            let peers = peer_info.into_iter().map(PeerInfo).collect();
            Ok(peers)
        }
        #[cfg(not(feature = "p2p"))]
        {
            Err(async_graphql::Error::new(
                "Peering is disabled in this build, try using the `p2p` feature flag.",
            ))
        }
    }
}

#[derive(Default)]
pub struct NodeQuery {}

#[Object]
impl NodeQuery {
    #[graphql(complexity = "QUERY_COSTS.storage_read + child_complexity")]
    async fn node_info(&self, ctx: &Context<'_>) -> async_graphql::Result<NodeInfo> {
        let config = ctx.data_unchecked::<GraphQLConfig>();

        const VERSION: &str = env!("CARGO_PKG_VERSION");

        Ok(NodeInfo {
            utxo_validation: config.utxo_validation,
            vm_backtrace: config.vm_backtrace,
            max_tx: (config.max_tx as u64).into(),
            max_depth: (config.max_txpool_depth as u64).into(),
            node_version: VERSION.to_owned(),
        })
    }
}

struct PeerInfo(fuel_core_types::services::p2p::PeerInfo);

#[Object]
impl PeerInfo {
    /// The libp2p peer id
    async fn id(&self) -> String {
        self.0.id.to_string()
    }

    /// The advertised multi-addrs that can be used to connect to this peer
    async fn addresses(&self) -> Vec<String> {
        self.0.peer_addresses.iter().cloned().collect()
    }

    /// The self-reported version of the client the peer is using
    async fn client_version(&self) -> Option<String> {
        self.0.client_version.clone()
    }

    /// The last reported height of the peer
    async fn block_height(&self) -> Option<U32> {
        self.0
            .heartbeat_data
            .block_height
            .map(|height| (*height).into())
    }

    /// The last heartbeat from this peer in unix epoch time ms
    async fn last_heartbeat_ms(&self) -> U64 {
        let time = self.0.heartbeat_data.last_heartbeat;
        let time = time
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        U64(time.try_into().unwrap_or_default())
    }

    /// The internal fuel p2p reputation of this peer
    async fn app_score(&self) -> f64 {
        self.0.app_score
    }
}
