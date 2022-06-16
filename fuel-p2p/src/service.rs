use crate::codecs::bincode::BincodeCodec;
use crate::{
    behavior::{FuelBehaviour, FuelBehaviourEvent},
    config::{build_transport, P2PConfig},
    gossipsub::messages::GossipsubMessage as FuelGossipsubMessage,
    peer_info::PeerInfo,
    request_response::messages::{
        ReqResNetworkError, RequestError, RequestMessage, ResponseError, ResponseMessage,
    },
};
use futures::prelude::*;
use libp2p::{
    gossipsub::{error::PublishError, MessageId, Sha256Topic, Topic},
    identity::Keypair,
    multiaddr::Protocol,
    request_response::RequestId,
    swarm::SwarmEvent,
    Multiaddr, PeerId, Swarm,
};
use rand::Rng;
use std::{collections::HashMap, error::Error};
use tokio::sync::oneshot;
use tracing::warn;

pub type GossipTopic = Sha256Topic;

/// Listens to the events on the p2p network
/// And forwards them to the Orchestrator
pub struct FuelP2PService {
    /// Store the local peer id
    pub local_peer_id: PeerId,
    /// Swarm handler for FuelBehaviour
    swarm: Swarm<FuelBehaviour<BincodeCodec>>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum FuelP2PEvent {
    Behaviour(FuelBehaviourEvent),
    NewListenAddr(Multiaddr),
    RequestMessage(RequestMessage),
}

impl FuelP2PService {
    pub async fn new(local_keypair: Keypair, config: P2PConfig) -> Result<Self, Box<dyn Error>> {
        let local_peer_id = PeerId::from(local_keypair.public());

        // configure and build P2P Serivce
        let transport = build_transport(local_keypair.clone()).await;
        let behaviour = FuelBehaviour::new(
            local_keypair,
            &config,
            BincodeCodec::new(config.max_block_size),
        );
        let mut swarm = Swarm::new(transport, behaviour, local_peer_id);

        // set up node's address to listen on
        let listen_multiaddr = {
            let mut m = Multiaddr::from(config.address);
            m.push(Protocol::Tcp(config.tcp_port));
            m
        };

        // subscribe to gossipsub topics with the network name suffix
        for topic in config.topics {
            let t = Topic::new(format!("{}/{}", topic, config.network_name));
            swarm.behaviour_mut().subscribe_to_topic(&t).unwrap();
        }

        // start listening at the given address
        swarm.listen_on(listen_multiaddr)?;

        Ok(Self {
            swarm,
            local_peer_id,
        })
    }

    pub fn get_peer_info(&self, peer_id: PeerId) -> Option<&PeerInfo> {
        self.swarm.behaviour().get_peer_info(&peer_id)
    }

    pub fn get_peers(&self) -> &HashMap<PeerId, PeerInfo> {
        self.swarm.behaviour().get_peers()
    }

    pub fn subscribe_to_topic(&mut self, topic: &GossipTopic) -> bool {
        match self.swarm.behaviour_mut().subscribe_to_topic(topic) {
            Ok(value) => value,
            Err(e) => {
                warn!(target: "fuel-libp2p", "Failed to subscribe to topic: {:?} with error: {:?}", topic, e);
                false
            }
        }
    }

    pub fn unsubscribe_from_topic(&mut self, topic: &GossipTopic) -> bool {
        match self.swarm.behaviour_mut().unsubscribe_from_topic(topic) {
            Ok(value) => value,
            Err(e) => {
                warn!(target: "fuel-libp2p", "Failed to unsubscribe from topic: {:?} with error: {:?}", topic, e);
                false
            }
        }
    }

    pub fn publish_message(
        &mut self,
        topic: GossipTopic,
        message: FuelGossipsubMessage,
    ) -> Result<MessageId, PublishError> {
        self.swarm.behaviour_mut().publish_message(topic, message)
    }

    pub async fn next_event(&mut self) -> FuelP2PEvent {
        loop {
            match self.swarm.select_next_some().await {
                SwarmEvent::Behaviour(fuel_behaviour) => {
                    return FuelP2PEvent::Behaviour(fuel_behaviour)
                }
                SwarmEvent::NewListenAddr { address, .. } => {
                    return FuelP2PEvent::NewListenAddr(address)
                }
                _ => {}
            }
        }
    }

    /// Sends RequestMessage to a peer
    /// If the peer is not defined it will pick one at random
    pub fn send_request_msg(
        &mut self,
        peer_id: Option<PeerId>,
        message_request: RequestMessage,
        tx_channel: oneshot::Sender<Result<ResponseMessage, ReqResNetworkError>>,
    ) -> Result<RequestId, RequestError> {
        let peer_id = match peer_id {
            Some(peer_id) => peer_id,
            _ => {
                let connected_peers = self.get_peers();
                if connected_peers.is_empty() {
                    return Err(RequestError::NoPeersConnected);
                }
                let rand_index = rand::thread_rng().gen_range(0..connected_peers.len());
                *connected_peers.keys().nth(rand_index).unwrap()
            }
        };

        Ok(self
            .swarm
            .behaviour_mut()
            .send_request_msg(message_request, peer_id, tx_channel))
    }

    /// Sends ResponseMessage to a peer that requested the data
    pub fn send_response_msg(
        &mut self,
        request_id: RequestId,
        message: ResponseMessage,
    ) -> Result<(), ResponseError> {
        self.swarm
            .behaviour_mut()
            .send_response_msg(request_id, message)
    }
}

#[cfg(test)]
mod tests {
    use super::{FuelBehaviourEvent, FuelP2PService};
    use crate::request_response::messages::{RequestMessage, ResponseMessage};
    use crate::{
        config::P2PConfig, peer_info::PeerInfo, request_response::messages::ReqResNetworkError,
        service::FuelP2PEvent,
    };
    use ctor::ctor;
    use libp2p::{gossipsub::Topic, identity::Keypair};
    use std::{
        net::{IpAddr, Ipv4Addr},
        time::Duration,
    };
    use tokio::sync::{mpsc, oneshot};
    use tracing_attributes::instrument;
    use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter};

    /// helper function for building default testing config
    fn build_p2p_config(network_name: &str) -> P2PConfig {
        P2PConfig {
            network_name: network_name.into(),
            address: IpAddr::V4(Ipv4Addr::from([0, 0, 0, 0])),
            tcp_port: 4000,
            max_block_size: 100_000,
            bootstrap_nodes: vec![],
            enable_mdns: false,
            max_peers_connected: 50,
            allow_private_addresses: true,
            enable_random_walk: true,
            connection_idle_timeout: Some(Duration::from_secs(120)),
            topics: vec![],
            max_mesh_size: 12,
            min_mesh_size: 4,
            ideal_mesh_size: 6,
            set_request_timeout: None,
            set_connection_keep_alive: None,
            info_interval: Some(Duration::from_secs(3)),
            identify_interval: Some(Duration::from_secs(5)),
        }
    }

    /// Conditionally initializes tracing, depending if RUST_LOG env variable is set
    /// Logs to stderr & to a file
    #[ctor]
    fn initialize_tracing() {
        if std::env::var_os("RUST_LOG").is_some() {
            let log_file = tracing_appender::rolling::daily("./logs", "p2p_logfile");

            let subscriber = tracing_subscriber::registry()
                .with(EnvFilter::from_default_env())
                .with(fmt::Layer::new().with_writer(std::io::stderr))
                .with(
                    fmt::Layer::new()
                        .compact()
                        .pretty()
                        .with_ansi(false) // disabling terminal color fixes this issue: https://github.com/tokio-rs/tracing/issues/1817
                        .with_writer(log_file),
                );

            tracing::subscriber::set_global_default(subscriber)
                .expect("Unable to set a global subscriber");
        }
    }

    /// helper function for building FuelP2PService    
    async fn build_fuel_p2p_service(p2p_config: P2PConfig) -> FuelP2PService {
        let keypair = Keypair::generate_secp256k1();
        let fuel_p2p_service = FuelP2PService::new(keypair, p2p_config).await.unwrap();

        fuel_p2p_service
    }

    #[tokio::test]
    #[instrument]
    async fn p2p_service_works() {
        let mut fuel_p2p_service =
            build_fuel_p2p_service(build_p2p_config("p2p_service_works")).await;

        loop {
            match fuel_p2p_service.next_event().await {
                FuelP2PEvent::NewListenAddr(_address) => {
                    // listener address registered, we are good to go
                    break;
                }
                other_event => {
                    tracing::error!("Unexpected event: {:?}", other_event);
                    panic!("Unexpected event")
                }
            }
        }
    }

    // Simulates 2 p2p nodes that are on the same network and should connect via mDNS
    // without any additional bootstrapping
    #[tokio::test]
    #[instrument]
    async fn nodes_connected_via_mdns() {
        // Node A
        let mut p2p_config = build_p2p_config("nodes_connected_via_mdns");
        p2p_config.tcp_port = 4001;
        p2p_config.enable_mdns = true;
        let mut node_a = build_fuel_p2p_service(p2p_config.clone()).await;

        // Node B
        p2p_config.tcp_port = 4002;
        let mut node_b = build_fuel_p2p_service(p2p_config).await;

        loop {
            tokio::select! {
                node_b_event = node_b.next_event() => {
                    if let FuelP2PEvent::Behaviour(FuelBehaviourEvent::PeerConnected(_)) = node_b_event {
                        // successfully connected to Node B
                        break
                    }
                    tracing::info!("Node B Event: {:?}", node_b_event);
                },
                node_a_event = node_a.next_event() => {
                    tracing::info!("Node A Event: {:?}", node_a_event);
                }
            };
        }
    }

    // Simulates 3 p2p nodes, Node B & Node C are bootstrapped with Node A
    // Using Identify Protocol Node C should be able to identify and connect to Node B
    #[tokio::test]
    #[instrument]
    async fn nodes_connected_via_identify() {
        // Node A
        let mut p2p_config = build_p2p_config("nodes_connected_via_identify");
        p2p_config.tcp_port = 4003;
        let mut node_a = build_fuel_p2p_service(p2p_config.clone()).await;

        let node_a_address = match node_a.next_event().await {
            FuelP2PEvent::NewListenAddr(address) => Some(address),
            _ => None,
        };

        // Node B
        p2p_config.tcp_port = 4004;
        p2p_config.bootstrap_nodes = vec![(node_a.local_peer_id, node_a_address.clone().unwrap())];
        let mut node_b = build_fuel_p2p_service(p2p_config.clone()).await;

        // Node C
        p2p_config.tcp_port = 4005;
        let mut node_c = build_fuel_p2p_service(p2p_config).await;

        loop {
            tokio::select! {
                node_a_event = node_a.next_event() => {
                    tracing::info!("Node A Event: {:?}", node_a_event);
                },
                node_b_event = node_b.next_event() => {
                    tracing::info!("Node B Event: {:?}", node_b_event);
                },

                node_c_event = node_c.next_event() => {
                    if let FuelP2PEvent::Behaviour(FuelBehaviourEvent::PeerConnected(peer_id)) = node_c_event {
                        // we have connected to Node B!
                        if peer_id == node_b.local_peer_id {
                            break
                        }
                    }

                    tracing::info!("Node C Event: {:?}", node_c_event);
                }
            };
        }
    }

    // Simulates 2 p2p nodes that connect to each other and consequently exchange Peer Info
    #[tokio::test]
    #[instrument]
    async fn peer_info_updates_work() {
        // Node A
        let mut p2p_config = build_p2p_config("peer_info_updates_work");
        p2p_config.tcp_port = 4006;
        let mut node_a = build_fuel_p2p_service(p2p_config.clone()).await;

        let node_a_address = match node_a.next_event().await {
            FuelP2PEvent::NewListenAddr(address) => Some(address),
            _ => None,
        };

        // Node B
        p2p_config.tcp_port = 4007;
        p2p_config.bootstrap_nodes = vec![(node_a.local_peer_id, node_a_address.clone().unwrap())];
        let mut node_b = build_fuel_p2p_service(p2p_config).await;

        loop {
            tokio::select! {
                node_a_event = node_a.next_event() => {
                    if let FuelP2PEvent::Behaviour(FuelBehaviourEvent::PeerInfoUpdated(peer_id)) = node_a_event {
                        if let Some(PeerInfo { peer_addresses, latest_ping, client_version, .. }) = node_a.swarm.behaviour().get_peer_info(&peer_id) {
                            // Exits after it verifies that:
                            // 1. Peer Addresses are known
                            // 2. Client Version is known
                            // 3. Node has been pinged and responded with success
                            if !peer_addresses.is_empty() && client_version.is_some() && latest_ping.is_some() {
                                break;
                            }
                        }
                    }

                    tracing::info!("Node A Event: {:?}", node_a_event);
                },
                node_b_event = node_b.next_event() => {
                    tracing::info!("Node B Event: {:?}", node_b_event);
                }
            };
        }
    }

    #[tokio::test]
    #[instrument]
    async fn gossipsub_exchanges_messages() {
        use crate::gossipsub::messages::GossipsubMessage as FuelGossipsubMessage;
        use fuel_core_interfaces::common::fuel_tx::Transaction;

        let mut p2p_config = build_p2p_config("gossipsub_exchanges_messages");
        let topics = vec!["create_tx".into(), "send_tx".into()];
        let selected_topic = Topic::new(format!("{}/{}", topics[0], p2p_config.network_name));
        let mut message_sent = false;

        // Node A
        p2p_config.tcp_port = 4008;
        p2p_config.topics = topics.clone();
        let mut node_a = build_fuel_p2p_service(p2p_config.clone()).await;

        let node_a_address = match node_a.next_event().await {
            FuelP2PEvent::NewListenAddr(address) => Some(address),
            _ => None,
        };

        // Node B
        p2p_config.tcp_port = 4009;
        p2p_config.bootstrap_nodes = vec![(node_a.local_peer_id, node_a_address.clone().unwrap())];
        let mut node_b = build_fuel_p2p_service(p2p_config.clone()).await;

        loop {
            tokio::select! {
                node_a_event = node_a.next_event() => {
                    if let FuelP2PEvent::Behaviour(FuelBehaviourEvent::PeerInfoUpdated(peer_id)) = node_a_event {
                        if let Some(PeerInfo { peer_addresses, .. }) = node_a.swarm.behaviour().get_peer_info(&peer_id) {
                            // verifies that we've got at least a single peer address to send message to
                            if !peer_addresses.is_empty() && !message_sent  {
                                message_sent = true;
                                let default_tx = FuelGossipsubMessage::NewTx(Transaction::default());
                                node_a.publish_message(selected_topic.clone(), default_tx).unwrap();
                            }
                        }
                    }

                    tracing::info!("Node A Event: {:?}", node_a_event);
                },
                node_b_event = node_b.next_event() => {
                    if let FuelP2PEvent::Behaviour(FuelBehaviourEvent::GossipsubMessage { topic_hash, message, .. }) = node_b_event.clone() {
                        if topic_hash != selected_topic.hash() {
                            tracing::error!("Wrong topic hash, expected: {} - actual: {}", selected_topic.hash(), topic_hash);
                            panic!("Wrong Topic");
                        }

                        if let FuelGossipsubMessage::NewTx(message) = message {
                            if message != Transaction::default() {
                                tracing::error!("Wrong p2p message, expected: {:?} - actual: {:?}", FuelGossipsubMessage::NewTx(Transaction::default()), message);
                                panic!("Wrong Message")

                            }
                        }

                        break
                    }

                    tracing::info!("Node B Event: {:?}", node_b_event);
                }
            };
        }
    }

    #[tokio::test]
    #[instrument]
    async fn request_response_works() {
        use fuel_core_interfaces::common::fuel_tx::Transaction;
        use fuel_core_interfaces::model::{FuelBlock, FuelBlockHeader};

        let mut p2p_config = build_p2p_config("request_response_works");

        // Node A
        p2p_config.tcp_port = 4010;
        let mut node_a = build_fuel_p2p_service(p2p_config.clone()).await;

        let node_a_address = match node_a.next_event().await {
            FuelP2PEvent::NewListenAddr(address) => Some(address),
            _ => None,
        };

        // Node B
        p2p_config.tcp_port = 4011;
        p2p_config.bootstrap_nodes = vec![(node_a.local_peer_id, node_a_address.clone().unwrap())];
        let mut node_b = build_fuel_p2p_service(p2p_config.clone()).await;

        let (tx_test_end, mut rx_test_end) = mpsc::channel(1);

        let mut request_sent = false;

        loop {
            tokio::select! {
                message_sent = rx_test_end.recv() => {
                    // we received a signal to end the test
                    assert_eq!(message_sent, Some(true), "Received wrong block height!");
                    break;
                }
                node_a_event = node_a.next_event() => {
                    if let FuelP2PEvent::Behaviour(FuelBehaviourEvent::PeerInfoUpdated(peer_id)) = node_a_event {
                        if let Some(PeerInfo { peer_addresses, .. }) = node_a.swarm.behaviour().get_peer_info(&peer_id) {
                            // 0. verifies that we've got at least a single peer address to request messsage from
                            if !peer_addresses.is_empty() && !request_sent {
                                request_sent = true;

                                // 1. Simulating Oneshot channel from the NetworkOrchestrator
                                let (tx_orchestrator, rx_orchestrator) = oneshot::channel();

                                let requested_block_height = RequestMessage::RequestBlock(0_u64.into());
                                assert!(node_a.send_request_msg(None, requested_block_height, tx_orchestrator).is_ok());

                                let tx_test_end = tx_test_end.clone();
                                tokio::spawn(async move {
                                    // 4. Simulating NetworkOrchestrator receving a message from Node B
                                    let response_message = rx_orchestrator.await;

                                    if let Ok(Ok(ResponseMessage::ResponseBlock(block))) = response_message {
                                        let _ = tx_test_end.send(block.header.height == 0_u64.into()).await;
                                    } else {
                                        tracing::error!("Orchestrator failed to receive a message: {:?}", response_message);
                                        panic!("Message not received successfully!")
                                    }

                                });
                            }
                        }
                    }

                    tracing::info!("Node A Event: {:?}", node_a_event);
                },
                node_b_event = node_b.next_event() => {
                    // 2. Node B recieves the RequestMessage from Node A initiated by the NetworkOrchestrator
                    if let FuelP2PEvent::Behaviour(FuelBehaviourEvent::RequestMessage{ request_id, .. }) = node_b_event {
                        let block = FuelBlock {
                            header: FuelBlockHeader::default(),
                            transactions: vec![Transaction::default(), Transaction::default(), Transaction::default(), Transaction::default(), Transaction::default()],
                        };

                        let _ = node_b.send_response_msg(request_id, ResponseMessage::ResponseBlock(block));
                    }

                    tracing::info!("Node B Event: {:?}", node_b_event);
                }
            };
        }
    }

    #[tokio::test]
    #[instrument]
    async fn req_res_outbound_timeout_works() {
        let mut p2p_config = build_p2p_config("req_res_outbound_timeout_works");

        // Node A
        p2p_config.tcp_port = 4012;
        // setup request timeout to 0 in order for the Request to fail
        p2p_config.set_request_timeout = Some(Duration::from_secs(0));
        let mut node_a = build_fuel_p2p_service(p2p_config.clone()).await;

        let node_a_address = match node_a.next_event().await {
            FuelP2PEvent::NewListenAddr(address) => Some(address),
            _ => None,
        };

        // Node B
        p2p_config.tcp_port = 4013;
        p2p_config.bootstrap_nodes = vec![(node_a.local_peer_id, node_a_address.clone().unwrap())];
        let mut node_b = build_fuel_p2p_service(p2p_config.clone()).await;

        let (tx_test_end, mut rx_test_end) = tokio::sync::mpsc::channel(1);

        // track the request sent in order to aviod duplicate sending
        let mut request_sent = false;

        loop {
            tokio::select! {
                node_a_event = node_a.next_event() => {
                    if let FuelP2PEvent::Behaviour(FuelBehaviourEvent::PeerInfoUpdated(peer_id)) = node_a_event {
                        if let Some(PeerInfo { peer_addresses, .. }) = node_a.swarm.behaviour().get_peer_info(&peer_id) {
                            // 0. verifies that we've got at least a single peer address to request messsage from
                            if !peer_addresses.is_empty() && !request_sent {
                                request_sent = true;

                                // 1. Simulating Oneshot channel from the NetworkOrchestrator
                                let (tx_orchestrator, rx_orchestrator) = oneshot::channel();

                                // 2a. there should be ZERO pending outbound requests in the table
                                assert_eq!(node_a.swarm.behaviour().get_outbound_requests_table().len(), 0);

                                // Request successfully sent
                                let requested_block_height = RequestMessage::RequestBlock(0_u64.into());
                                assert!(node_a.send_request_msg(None, requested_block_height, tx_orchestrator).is_ok());

                                // 2b. there should be ONE pending outbound requests in the table
                                assert_eq!(node_a.swarm.behaviour().get_outbound_requests_table().len(), 1);

                                let tx_test_end = tx_test_end.clone();

                                tokio::spawn(async move {
                                    // 3. Simulating NetworkOrchestrator receving a Timeout Error Message!
                                    if let Ok(Err(ReqResNetworkError::Timeout)) = rx_orchestrator.await {
                                        let _ = tx_test_end.send(()).await;
                                    }
                                });
                            }
                        }
                    }

                    tracing::info!("Node A Event: {:?}", node_a_event);
                },
                _ = rx_test_end.recv() => {
                    // we received a signal to end the test
                    // 4. there should be ZERO pending outbound requests in the table
                    // after the Outbound Request Failed with Timeout
                    assert_eq!(node_a.swarm.behaviour().get_outbound_requests_table().len(), 0);
                    break;
                },
                // will not receive the request at all
                node_b_event = node_b.next_event() => {
                    tracing::info!("Node B Event: {:?}", node_b_event);
                }
            };
        }
    }
}
