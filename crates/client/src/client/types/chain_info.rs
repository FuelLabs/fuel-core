use crate::client::{
    schema,
    types::Block,
};
use fuel_core_types::{
    fuel_tx::ConsensusParameters,
    services::p2p::{
        HeartbeatData,
        PeerId,
        PeerInfo,
    },
};
use std::{
    str::FromStr,
    time::{
        Duration,
        UNIX_EPOCH,
    },
};

pub struct ChainInfo {
    pub da_height: u64,
    pub name: String,
    pub peers: Vec<PeerInfo>,
    pub latest_block: Block,
    pub consensus_parameters: ConsensusParameters,
}

// GraphQL Translation

impl From<schema::chain::ChainInfo> for ChainInfo {
    fn from(value: schema::chain::ChainInfo) -> Self {
        Self {
            da_height: value.da_height.into(),
            name: value.name,
            peers: value.peers.into_iter().map(|info| info.into()).collect(),
            latest_block: value.latest_block.into(),
            consensus_parameters: value.consensus_parameters.into(),
        }
    }
}

impl From<schema::chain::PeerInfo> for PeerInfo {
    fn from(info: schema::chain::PeerInfo) -> Self {
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
