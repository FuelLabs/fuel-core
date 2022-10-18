use crate::{
    behavior::{
        FuelBehaviour,
        FuelBehaviourEvent,
    },
    codecs::NetworkCodec,
    config::{
        build_transport,
        P2PConfig,
    },
    discovery::DiscoveryEvent,
    gossipsub::{
        messages::{
            GossipsubBroadcastRequest,
            GossipsubMessage as FuelGossipsubMessage,
        },
        topics::GossipsubTopics,
    },
    peer_info::{
        PeerInfo,
        PeerInfoEvent,
    },
    request_response::messages::{
        IntermediateResponse,
        OutboundResponse,
        RequestError,
        RequestMessage,
        ResponseChannelItem,
        ResponseError,
        ResponseMessage,
    },
};
use futures::prelude::*;
use libp2p::{
    gossipsub::{
        error::PublishError,
        GossipsubEvent,
        MessageId,
        Topic,
        TopicHash,
    },
    multiaddr::Protocol,
    request_response::{
        RequestId,
        RequestResponseEvent,
        RequestResponseMessage,
        ResponseChannel,
    },
    swarm::{
        SwarmBuilder,
        SwarmEvent,
    },
    Multiaddr,
    PeerId,
    Swarm,
};
use rand::Rng;
use std::collections::HashMap;
use tracing::{
    debug,
    warn,
};

/// Listens to the events on the p2p network
/// And forwards them to the Orchestrator
pub struct FuelP2PService<Codec: NetworkCodec> {
    /// Store the local peer id
    pub local_peer_id: PeerId,
    /// Swarm handler for FuelBehaviour
    swarm: Swarm<FuelBehaviour<Codec>>,

    /// Holds the Sender(s) part of the Oneshot Channel from the NetworkOrchestrator
    /// Once the ResponseMessage is received from the p2p Network
    /// It will send it to the NetworkOrchestrator via its unique Sender    
    outbound_requests_table: HashMap<RequestId, ResponseChannelItem>,

    /// Holds the ResponseChannel(s) for the inbound requests from the p2p Network
    /// Once the Response is prepared by the NetworkOrchestrator
    /// It will send it to the specified Peer via its unique ResponseChannel    
    inbound_requests_table: HashMap<RequestId, ResponseChannel<IntermediateResponse>>,

    /// NetworkCodec used as <GossipsubCodec> for encoding and decoding of Gossipsub messages    
    network_codec: Codec,

    /// Stores additional p2p network info    
    network_metadata: NetworkMetadata,
}

/// Holds additional Network data for FuelBehavior
#[derive(Debug)]
struct NetworkMetadata {
    gossipsub_topics: GossipsubTopics,
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum FuelP2PEvent {
    GossipsubMessage {
        peer_id: PeerId,
        topic_hash: TopicHash,
        message: FuelGossipsubMessage,
    },
    RequestMessage {
        request_id: RequestId,
        request_message: RequestMessage,
    },
    PeerConnected(PeerId),
    PeerDisconnected(PeerId),
    PeerInfoUpdated(PeerId),
}

impl<Codec: NetworkCodec> FuelP2PService<Codec> {
    pub fn new(config: P2PConfig, codec: Codec) -> anyhow::Result<Self> {
        let local_peer_id = PeerId::from(config.local_keypair.public());

        // configure and build P2P Service
        let transport = build_transport(config.local_keypair.clone());
        let behaviour = FuelBehaviour::new(&config, codec.clone());
        let mut swarm = SwarmBuilder::new(transport, behaviour, local_peer_id)
            .executor(Box::new(|fut| {
                tokio::spawn(fut);
            }))
            .build();

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

        let gossipsub_topics = GossipsubTopics::new(&config.network_name);
        let network_metadata = NetworkMetadata { gossipsub_topics };

        Ok(Self {
            local_peer_id,
            swarm,
            network_codec: codec,
            outbound_requests_table: HashMap::default(),
            inbound_requests_table: HashMap::default(),
            network_metadata,
        })
    }

    pub fn get_peers(&self) -> &HashMap<PeerId, PeerInfo> {
        self.swarm.behaviour().get_peers()
    }

    pub fn publish_message(
        &mut self,
        message: GossipsubBroadcastRequest,
    ) -> Result<MessageId, PublishError> {
        let topic = self
            .network_metadata
            .gossipsub_topics
            .get_gossipsub_topic(&message);

        match self.network_codec.encode(message) {
            Ok(encoded_data) => self
                .swarm
                .behaviour_mut()
                .publish_message(topic, encoded_data),
            Err(e) => Err(PublishError::TransformFailed(e)),
        }
    }

    /// Sends RequestMessage to a peer
    /// If the peer is not defined it will pick one at random
    pub fn send_request_msg(
        &mut self,
        peer_id: Option<PeerId>,
        message_request: RequestMessage,
        channel_item: ResponseChannelItem,
    ) -> Result<RequestId, RequestError> {
        let peer_id = match peer_id {
            Some(peer_id) => peer_id,
            _ => {
                let connected_peers = self.get_peers();
                if connected_peers.is_empty() {
                    return Err(RequestError::NoPeersConnected)
                }
                let rand_index = rand::thread_rng().gen_range(0..connected_peers.len());
                *connected_peers.keys().nth(rand_index).unwrap()
            }
        };

        let request_id = self
            .swarm
            .behaviour_mut()
            .send_request_msg(message_request, peer_id);

        self.outbound_requests_table
            .insert(request_id, channel_item);

        Ok(request_id)
    }

    /// Sends ResponseMessage to a peer that requested the data
    pub fn send_response_msg(
        &mut self,
        request_id: RequestId,
        message: OutboundResponse,
    ) -> Result<(), ResponseError> {
        match (
            self.network_codec.convert_to_intermediate(&message),
            self.inbound_requests_table.remove(&request_id),
        ) {
            (Ok(message), Some(channel)) => {
                if self
                    .swarm
                    .behaviour_mut()
                    .send_response_msg(channel, message)
                    .is_err()
                {
                    debug!("Failed to send ResponseMessage for {:?}", request_id);
                    return Err(ResponseError::SendingResponseFailed)
                }
            }
            (Ok(_), None) => {
                debug!("ResponseChannel for {:?} does not exist!", request_id);
                return Err(ResponseError::ResponseChannelDoesNotExist)
            }
            (Err(e), _) => {
                debug!("Failed to convert to IntermediateResponse with {:?}", e);
                return Err(ResponseError::ConversionToIntermediateFailed)
            }
        }

        Ok(())
    }

    pub async fn next_event(&mut self) -> FuelP2PEvent {
        loop {
            if let SwarmEvent::Behaviour(fuel_behaviour) =
                self.swarm.select_next_some().await
            {
                if let Some(event) = self.handle_behaviour_event(fuel_behaviour) {
                    return event
                }
            }
        }
    }

    fn handle_behaviour_event(
        &mut self,
        event: FuelBehaviourEvent,
    ) -> Option<FuelP2PEvent> {
        match event {
            FuelBehaviourEvent::Discovery(discovery_event) => match discovery_event {
                DiscoveryEvent::Connected(peer_id, addresses) => {
                    self.swarm
                        .behaviour_mut()
                        .add_addresses_to_peer_info(&peer_id, addresses);

                    return Some(FuelP2PEvent::PeerConnected(peer_id))
                }
                DiscoveryEvent::Disconnected(peer_id) => {
                    return Some(FuelP2PEvent::PeerDisconnected(peer_id))
                }
                _ => {}
            },
            FuelBehaviourEvent::Gossipsub(gossipsub_event) => {
                if let GossipsubEvent::Message {
                    propagation_source,
                    message,
                    ..
                } = gossipsub_event
                {
                    if let Some(correct_topic) = self
                        .network_metadata
                        .gossipsub_topics
                        .get_gossipsub_tag(&message.topic)
                    {
                        match self.network_codec.decode(&message.data, correct_topic) {
                            Ok(decoded_message) => {
                                return Some(FuelP2PEvent::GossipsubMessage {
                                    peer_id: propagation_source,
                                    topic_hash: message.topic,
                                    message: decoded_message,
                                })
                            }
                            Err(err) => {
                                warn!(target: "fuel-libp2p", "Failed to decode a message: {:?} with error: {:?}", &message.data, err);
                            }
                        }
                    } else {
                        warn!(target: "fuel-libp2p", "GossipTopicTag does not exist for {:?}", &message.topic);
                    }
                }
            }

            FuelBehaviourEvent::PeerInfo(peer_info_event) => match peer_info_event {
                PeerInfoEvent::PeerIdentified { peer_id, addresses } => {
                    self.swarm
                        .behaviour_mut()
                        .add_addresses_to_discovery(&peer_id, addresses);
                }
                PeerInfoEvent::PeerInfoUpdated { peer_id } => {
                    return Some(FuelP2PEvent::PeerInfoUpdated(peer_id))
                }
            },
            FuelBehaviourEvent::RequestResponse(req_res_event) => match req_res_event {
                RequestResponseEvent::Message { message, .. } => match message {
                    RequestResponseMessage::Request {
                        request,
                        channel,
                        request_id,
                    } => {
                        self.inbound_requests_table.insert(request_id, channel);

                        return Some(FuelP2PEvent::RequestMessage {
                            request_id,
                            request_message: request,
                        })
                    }
                    RequestResponseMessage::Response {
                        request_id,
                        response,
                    } => {
                        match (
                            self.outbound_requests_table.remove(&request_id),
                            self.network_codec.convert_to_response(&response),
                        ) {
                            (
                                Some(ResponseChannelItem::ResponseBlock(channel)),
                                Ok(ResponseMessage::ResponseBlock(block)),
                            ) => {
                                if channel.send(block).is_err() {
                                    debug!(
                                        "Failed to send through the channel for {:?}",
                                        request_id
                                    );
                                }
                            }

                            (Some(_), Err(e)) => {
                                debug!("Failed to convert IntermediateResponse into a ResponseMessage {:?} with {:?}", response, e);
                            }
                            (None, Ok(_)) => {
                                debug!("Send channel not found for {:?}", request_id);
                            }
                            _ => {}
                        }
                    }
                },
                RequestResponseEvent::InboundFailure {
                    peer,
                    error,
                    request_id,
                } => {
                    debug!("RequestResponse inbound error for peer: {:?} with id: {:?} and error: {:?}", peer, request_id, error);
                }
                RequestResponseEvent::OutboundFailure {
                    peer,
                    error,
                    request_id,
                } => {
                    debug!("RequestResponse outbound error for peer: {:?} with id: {:?} and error: {:?}", peer, request_id, error);

                    let _ = self.outbound_requests_table.remove(&request_id);
                }
                _ => {}
            },
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::FuelP2PService;
    use crate::{
        codecs::bincode::BincodeCodec,
        config::P2PConfig,
        gossipsub::{
            messages::{
                GossipsubBroadcastRequest,
                GossipsubMessage,
            },
            topics::{
                GossipTopic,
                CON_VOTE_GOSSIP_TOPIC,
                NEW_BLOCK_GOSSIP_TOPIC,
                NEW_TX_GOSSIP_TOPIC,
            },
        },
        peer_info::PeerInfo,
        request_response::messages::{
            OutboundResponse,
            RequestMessage,
            ResponseChannelItem,
        },
        service::FuelP2PEvent,
    };
    use ctor::ctor;
    use fuel_core_interfaces::{
        common::fuel_tx::Transaction,
        model::{
            ConsensusVote,
            FuelBlock,
            FuelBlockConsensus,
            PartialFuelBlockHeader,
        },
    };
    use futures::StreamExt;
    use libp2p::{
        gossipsub::Topic,
        identity::Keypair,
        swarm::SwarmEvent,
        Multiaddr,
        PeerId,
    };
    use std::{
        sync::Arc,
        time::Duration,
    };
    use tokio::sync::{
        mpsc,
        oneshot,
    };
    use tracing_attributes::instrument;
    use tracing_subscriber::{
        fmt,
        layer::SubscriberExt,
        EnvFilter,
    };

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
    async fn build_fuel_p2p_service(
        mut p2p_config: P2PConfig,
    ) -> FuelP2PService<BincodeCodec> {
        p2p_config.local_keypair = Keypair::generate_secp256k1(); // change keypair for each Node
        let max_block_size = p2p_config.max_block_size;

        FuelP2PService::new(p2p_config, BincodeCodec::new(max_block_size)).unwrap()
    }

    /// attaches PeerId to the Multiaddr
    fn build_bootstrap_node(peer_id: PeerId, address: Multiaddr) -> Multiaddr {
        format!("{}/p2p/{}", address, peer_id).parse().unwrap()
    }

    #[tokio::test]
    #[instrument]
    async fn p2p_service_works() {
        let mut fuel_p2p_service =
            build_fuel_p2p_service(P2PConfig::default_with_network("p2p_service_works"))
                .await;

        loop {
            match fuel_p2p_service.swarm.select_next_some().await {
                SwarmEvent::NewListenAddr { .. } => {
                    // listener address registered, we are good to go
                    break
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
        let mut p2p_config = P2PConfig::default_with_network("nodes_connected_via_mdns");
        p2p_config.enable_mdns = true;
        let mut node_a = build_fuel_p2p_service(p2p_config.clone()).await;

        // Node B
        let mut node_b = build_fuel_p2p_service(p2p_config).await;

        loop {
            tokio::select! {
                node_b_event = node_b.next_event() => {
                    if let FuelP2PEvent::PeerConnected(_) = node_b_event {
                        // successfully connected to Node B
                        break
                    }
                    tracing::info!("Node B Event: {:?}", node_b_event);
                },
                node_a_event = node_a.swarm.select_next_some() => {
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
        let mut p2p_config =
            P2PConfig::default_with_network("nodes_connected_via_identify");
        let mut node_a = build_fuel_p2p_service(p2p_config.clone()).await;

        let node_a_address = match node_a.swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => Some(address),
            _ => None,
        };

        // Node B
        p2p_config.bootstrap_nodes = vec![build_bootstrap_node(
            node_a.local_peer_id,
            node_a_address.clone().unwrap(),
        )];
        let mut node_b = build_fuel_p2p_service(p2p_config.clone()).await;

        // Node C
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
                    if let FuelP2PEvent::PeerConnected(peer_id) = node_c_event {
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
        let mut p2p_config = P2PConfig::default_with_network("peer_info_updates_work");
        let mut node_a = build_fuel_p2p_service(p2p_config.clone()).await;

        let node_a_address = match node_a.swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => Some(address),
            _ => None,
        };

        // Node B
        p2p_config.bootstrap_nodes = vec![build_bootstrap_node(
            node_a.local_peer_id,
            node_a_address.clone().unwrap(),
        )];
        let mut node_b = build_fuel_p2p_service(p2p_config).await;

        loop {
            tokio::select! {
                node_a_event = node_a.next_event() => {
                    if let FuelP2PEvent::PeerInfoUpdated(peer_id) = node_a_event {
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
    async fn gossipsub_broadcast_tx() {
        gossipsub_broadcast(GossipsubBroadcastRequest::NewTx(Arc::new(
            Transaction::default(),
        )))
        .await;
    }

    #[tokio::test]
    #[instrument]
    async fn gossipsub_broadcast_vote() {
        gossipsub_broadcast(GossipsubBroadcastRequest::ConsensusVote(Arc::new(
            ConsensusVote::default(),
        )))
        .await;
    }

    #[tokio::test]
    #[instrument]
    async fn gossipsub_broadcast_block() {
        gossipsub_broadcast(GossipsubBroadcastRequest::NewBlock(Arc::new(
            FuelBlock::default(),
        )))
        .await;
    }

    /// Reusable helper function for Broadcasting Gossipsub requests
    async fn gossipsub_broadcast(broadcast_request: GossipsubBroadcastRequest) {
        let mut p2p_config =
            P2PConfig::default_with_network("gossipsub_exchanges_messages");
        let topics = vec![
            NEW_TX_GOSSIP_TOPIC.into(),
            NEW_BLOCK_GOSSIP_TOPIC.into(),
            CON_VOTE_GOSSIP_TOPIC.into(),
        ];

        let selected_topic: GossipTopic = {
            let topic = match broadcast_request {
                GossipsubBroadcastRequest::ConsensusVote(_) => CON_VOTE_GOSSIP_TOPIC,
                GossipsubBroadcastRequest::NewBlock(_) => NEW_BLOCK_GOSSIP_TOPIC,
                GossipsubBroadcastRequest::NewTx(_) => NEW_TX_GOSSIP_TOPIC,
            };

            Topic::new(format!("{}/{}", topic, p2p_config.network_name))
        };

        let mut message_sent = false;

        // Node A
        p2p_config.topics = topics.clone();
        let mut node_a = build_fuel_p2p_service(p2p_config.clone()).await;

        let node_a_address = match node_a.swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => Some(address),
            _ => None,
        };

        // Node B
        p2p_config.bootstrap_nodes = vec![build_bootstrap_node(
            node_a.local_peer_id,
            node_a_address.clone().unwrap(),
        )];
        let mut node_b = build_fuel_p2p_service(p2p_config.clone()).await;

        loop {
            tokio::select! {
                node_a_event = node_a.next_event() => {
                    if let FuelP2PEvent::PeerInfoUpdated(peer_id) = node_a_event {
                        if let Some(PeerInfo { peer_addresses, .. }) = node_a.swarm.behaviour().get_peer_info(&peer_id) {
                            // verifies that we've got at least a single peer address to send message to
                            if !peer_addresses.is_empty() && !message_sent  {
                                message_sent = true;
                                let broadcast_request = broadcast_request.clone();
                                node_a.publish_message(broadcast_request).unwrap();
                            }
                        }
                    }

                    tracing::info!("Node A Event: {:?}", node_a_event);
                },
                node_b_event = node_b.next_event() => {
                    if let FuelP2PEvent::GossipsubMessage { topic_hash, message, .. } = node_b_event.clone() {
                        if topic_hash != selected_topic.hash() {
                            tracing::error!("Wrong topic hash, expected: {} - actual: {}", selected_topic.hash(), topic_hash);
                            panic!("Wrong Topic");
                        }

                        // received value should match sent value
                        match &message {
                            GossipsubMessage::NewTx(tx) => {
                                if tx != &Transaction::default() {
                                    tracing::error!("Wrong p2p message {:?}", message);
                                    panic!("Wrong GossipsubMessage")
                                }
                            }
                            GossipsubMessage::NewBlock(block) => {
                                    if block.header().height() != FuelBlock::default().header().height() {
                                    tracing::error!("Wrong p2p message {:?}", message);
                                    panic!("Wrong GossipsubMessage")
                                }
                            }
                            GossipsubMessage::ConsensusVote(vote) => {
                                if vote != &ConsensusVote::default() {
                                    tracing::error!("Wrong p2p message {:?}", message);
                                    panic!("Wrong GossipsubMessage")
                                }
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
        use fuel_core_interfaces::{
            common::fuel_tx::Transaction,
            model::{
                FuelBlock,
                FuelBlockPoAConsensus,
                SealedFuelBlock,
            },
        };

        let mut p2p_config = P2PConfig::default_with_network("request_response_works");

        // Node A
        let mut node_a = build_fuel_p2p_service(p2p_config.clone()).await;

        let node_a_address = match node_a.swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => Some(address),
            _ => None,
        };

        // Node B
        p2p_config.bootstrap_nodes = vec![build_bootstrap_node(
            node_a.local_peer_id,
            node_a_address.clone().unwrap(),
        )];
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
                    if let FuelP2PEvent::PeerInfoUpdated(peer_id) = node_a_event {
                        if let Some(PeerInfo { peer_addresses, .. }) = node_a.swarm.behaviour().get_peer_info(&peer_id) {
                            // 0. verifies that we've got at least a single peer address to request message from
                            if !peer_addresses.is_empty() && !request_sent {
                                request_sent = true;

                                // 1. Simulating Oneshot channel from the NetworkOrchestrator
                                let (tx_orchestrator, rx_orchestrator) = oneshot::channel();

                                let requested_block_height = RequestMessage::RequestBlock(0_u64.into());
                                assert!(node_a.send_request_msg(None, requested_block_height, ResponseChannelItem::ResponseBlock(tx_orchestrator)).is_ok());

                                let tx_test_end = tx_test_end.clone();
                                tokio::spawn(async move {
                                    // 4. Simulating NetworkOrchestrator receiving a message from Node B
                                    let response_message = rx_orchestrator.await;

                                    if let Ok(sealed_block) = response_message {
                                        let _ = tx_test_end.send(*sealed_block.header().height() == 0_u64.into()).await;
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
                    // 2. Node B receives the RequestMessage from Node A initiated by the NetworkOrchestrator
                    if let FuelP2PEvent::RequestMessage{ request_id, .. } = node_b_event {
                        let block = FuelBlock::new(
                            PartialFuelBlockHeader::default(),
                            vec![Transaction::default(), Transaction::default(), Transaction::default(), Transaction::default(), Transaction::default()],
                            &[]
                        );

                        let sealed_block = SealedFuelBlock {
                            block,
                            consensus: FuelBlockConsensus::PoA(FuelBlockPoAConsensus::new(Default::default())),
                        };

                        let _ = node_b.send_response_msg(request_id, OutboundResponse::ResponseBlock(Arc::new(sealed_block)));
                    }

                    tracing::info!("Node B Event: {:?}", node_b_event);
                }
            };
        }
    }

    #[tokio::test]
    #[instrument]
    async fn req_res_outbound_timeout_works() {
        let mut p2p_config =
            P2PConfig::default_with_network("req_res_outbound_timeout_works");

        // Node A
        // setup request timeout to 0 in order for the Request to fail
        p2p_config.set_request_timeout = Duration::from_secs(0);
        let mut node_a = build_fuel_p2p_service(p2p_config.clone()).await;

        let node_a_address = match node_a.swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => Some(address),
            _ => None,
        };

        // Node B
        p2p_config.bootstrap_nodes = vec![build_bootstrap_node(
            node_a.local_peer_id,
            node_a_address.clone().unwrap(),
        )];
        let mut node_b = build_fuel_p2p_service(p2p_config.clone()).await;

        let (tx_test_end, mut rx_test_end) = tokio::sync::mpsc::channel(1);

        // track the request sent in order to avoid duplicate sending
        let mut request_sent = false;

        loop {
            tokio::select! {
                node_a_event = node_a.next_event() => {
                    if let FuelP2PEvent::PeerInfoUpdated(peer_id) = node_a_event {
                        if let Some(PeerInfo { peer_addresses, .. }) = node_a.swarm.behaviour().get_peer_info(&peer_id) {
                            // 0. verifies that we've got at least a single peer address to request message from
                            if !peer_addresses.is_empty() && !request_sent {
                                request_sent = true;

                                // 1. Simulating Oneshot channel from the NetworkOrchestrator
                                let (tx_orchestrator, rx_orchestrator) = oneshot::channel();

                                // 2a. there should be ZERO pending outbound requests in the table
                                assert_eq!(node_a.outbound_requests_table.len(), 0);

                                // Request successfully sent
                                let requested_block_height = RequestMessage::RequestBlock(0_u64.into());
                                assert!(node_a.send_request_msg(None, requested_block_height, ResponseChannelItem::ResponseBlock(tx_orchestrator)).is_ok());

                                // 2b. there should be ONE pending outbound requests in the table
                                assert_eq!(node_a.outbound_requests_table.len(), 1);

                                let tx_test_end = tx_test_end.clone();

                                tokio::spawn(async move {
                                    // 3. Simulating NetworkOrchestrator receiving a Timeout Error Message!
                                    if (rx_orchestrator.await).is_err() {
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
                    assert_eq!(node_a.outbound_requests_table.len(), 0);
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
