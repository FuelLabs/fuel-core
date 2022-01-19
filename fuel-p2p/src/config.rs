use std::{net::IpAddr, path::PathBuf};

use libp2p::Multiaddr;

#[derive(Clone, Debug)]
pub struct P2PConfig {
    /// path to the private key
    pub key_path: PathBuf,

    /// IP address for p2p to listen on
    pub address: IpAddr,

    /// The TCP port that libp2p listens on
    pub tcp_port: u16,

    /// List of libp2p peer nodes to connect to
    pub peers: Vec<Multiaddr>,

    /// topics to subscribe to
    pub topics: Vec<FuelP2PTopics>,
}

#[derive(Clone, Debug, Copy)]
pub enum FuelP2PTopics {}
