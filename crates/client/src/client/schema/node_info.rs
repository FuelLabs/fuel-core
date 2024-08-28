use crate::client::schema::{
    schema,
    U32,
    U64,
};
#[cfg(feature = "std")]
use fuel_core_types::services::p2p::{
    HeartbeatData,
    PeerId,
};
#[cfg(feature = "std")]
use std::{
    str::FromStr,
    time::{
        Duration,
        UNIX_EPOCH,
    },
};

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct NodeInfo {
    pub utxo_validation: bool,
    pub vm_backtrace: bool,
    pub max_tx: U64,
    pub max_depth: U64,
    pub node_version: String,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl", graphql_type = "Query")]
pub struct QueryNodeInfo {
    pub node_info: NodeInfo,
}

// Use a separate GQL query for showing peer info, as the endpoint is bulky and may return an error
// if the `p2p` feature is disabled.

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl", graphql_type = "NodeInfo")]
pub struct PeersInfo {
    pub peers: Vec<PeerInfo>,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl", graphql_type = "Query")]
pub struct QueryPeersInfo {
    pub node_info: PeersInfo,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct PeerInfo {
    pub id: String,
    pub addresses: Vec<String>,
    pub client_version: Option<String>,
    pub block_height: Option<U32>,
    pub last_heartbeat_ms: U64,
    pub app_score: f64,
}

#[cfg(feature = "std")]
impl From<PeerInfo> for fuel_core_types::services::p2p::PeerInfo {
    fn from(info: PeerInfo) -> Self {
        Self {
            id: PeerId::from_str(info.id.as_str()).unwrap_or_default(),
            peer_addresses: info.addresses.into_iter().collect(),
            client_version: info.client_version,
            heartbeat_data: HeartbeatData {
                block_height: info.block_height.map(|h| h.0.into()),
                last_heartbeat: UNIX_EPOCH
                    .checked_add(Duration::from_millis(info.last_heartbeat_ms.0))
                    .unwrap_or(UNIX_EPOCH),
            },
            app_score: info.app_score,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_info_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = QueryNodeInfo::build(());
        insta::assert_snapshot!(operation.query)
    }

    #[test]
    fn peers_info_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = QueryPeersInfo::build(());
        insta::assert_snapshot!(operation.query)
    }
}
