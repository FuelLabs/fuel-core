use crate::{
    behavior::{
        FuelBehaviour,
        FuelBehaviourEvent,
    },
    codecs::NetworkCodec,
    config::{
        build_transport,
        Config,
    },
    discovery::DiscoveryEvent,
    gossipsub::{
        messages::{
            GossipsubBroadcastRequest,
            GossipsubMessage as FuelGossipsubMessage,
        },
        topics::GossipsubTopics,
    },
    peer_manager::{
        PeerInfoEvent,
        PeerManagerBehaviour,
    },
    request_response::messages::{
        NetworkResponse,
        OutboundResponse,
        RequestError,
        RequestMessage,
        ResponseChannelItem,
        ResponseError,
        ResponseMessage,
    },
};
use fuel_core_metrics::p2p_metrics::P2P_METRICS;
use fuel_core_types::blockchain::primitives::BlockHeight;
use futures::prelude::*;
use libp2p::{
    gossipsub::{
        error::PublishError,
        GossipsubEvent,
        MessageAcceptance,
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
        AddressScore,
        ConnectionLimits,
        SwarmBuilder,
        SwarmEvent,
    },
    Multiaddr,
    PeerId,
    Swarm,
};
use rand::seq::IteratorRandom;
use std::collections::HashMap;
use tracing::{
    debug,
    info,
    warn,
};

/// Listens to the events on the p2p network
/// And forwards them to the Orchestrator
pub struct FuelP2PService<Codec: NetworkCodec> {
    /// Store the local peer id
    pub local_peer_id: PeerId,

    /// IP address for Swarm to listen on
    local_address: std::net::IpAddr,

    /// The TCP port that Swarm listens on
    tcp_port: u16,

    /// Swarm handler for FuelBehaviour
    swarm: Swarm<FuelBehaviour<Codec>>,

    /// Holds the Sender(s) part of the Oneshot Channel from the NetworkOrchestrator
    /// Once the ResponseMessage is received from the p2p Network
    /// It will send it to the NetworkOrchestrator via its unique Sender    
    outbound_requests_table: HashMap<RequestId, ResponseChannelItem>,

    /// Holds the ResponseChannel(s) for the inbound requests from the p2p Network
    /// Once the Response is prepared by the NetworkOrchestrator
    /// It will send it to the specified Peer via its unique ResponseChannel    
    inbound_requests_table: HashMap<RequestId, ResponseChannel<NetworkResponse>>,

    /// NetworkCodec used as <GossipsubCodec> for encoding and decoding of Gossipsub messages    
    network_codec: Codec,

    /// Stores additional p2p network info    
    network_metadata: NetworkMetadata,

    /// Whether or not metrics collection is enabled
    metrics: bool,
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
        message_id: MessageId,
        topic_hash: TopicHash,
        message: FuelGossipsubMessage,
    },
    RequestMessage {
        request_id: RequestId,
        request_message: RequestMessage,
    },
    PeerConnected(PeerId),
    PeerDisconnected(PeerId),
    PeerInfoUpdated {
        peer_id: PeerId,
        block_height: BlockHeight,
    },
}

impl<Codec: NetworkCodec> FuelP2PService<Codec> {
    pub fn new(config: Config, codec: Codec) -> Self {
        let local_peer_id = PeerId::from(config.keypair.public());

        // configure and build P2P Service
        let (transport, connection_state) = build_transport(&config);
        let behaviour = FuelBehaviour::new(&config, codec.clone(), connection_state);

        let total_connections = {
            // Reserved nodes do not count against the configured peer input/output limits.
            let total_peers =
                config.max_peers_connected + config.reserved_nodes.len() as u32;

            total_peers * config.max_connections_per_peer
        };

        let max_established_incoming = {
            if config.reserved_nodes_only_mode {
                // If this is a guarded node,
                // it should not receive any incoming connection requests.
                // Rather, it will send outgoing connection requests to its reserved nodes
                0
            } else {
                total_connections / 2
            }
        };

        let connection_limits = ConnectionLimits::default()
            .with_max_established_incoming(Some(max_established_incoming))
            .with_max_established_per_peer(Some(config.max_connections_per_peer))
            // libp2p does not manage how many different peers we're connected to
            // it only takes care that there are 'N' amount of connections established.
            // Our `PeerManagerBehaviour` will keep track of different peers connected
            // and disconnect any surplus peers
            .with_max_established(Some(total_connections));

        let mut swarm =
            SwarmBuilder::with_tokio_executor(transport, behaviour, local_peer_id)
                .connection_limits(connection_limits)
                .build();

        // subscribe to gossipsub topics with the network name suffix
        for topic in config.topics {
            let t = Topic::new(format!("{}/{}", topic, config.network_name));
            swarm.behaviour_mut().subscribe_to_topic(&t).unwrap();
        }

        let gossipsub_topics = GossipsubTopics::new(&config.network_name);
        let network_metadata = NetworkMetadata { gossipsub_topics };

        let metrics = config.metrics;

        if let Some(public_address) = config.public_address {
            let _ = swarm.add_external_address(public_address, AddressScore::Infinite);
        }

        Self {
            local_peer_id,
            local_address: config.address,
            tcp_port: config.tcp_port,
            swarm,
            network_codec: codec,
            outbound_requests_table: HashMap::default(),
            inbound_requests_table: HashMap::default(),
            network_metadata,
            metrics,
        }
    }

    pub fn start(&mut self) -> anyhow::Result<()> {
        // set up node's address to listen on
        let listen_multiaddr = {
            let mut m = Multiaddr::from(self.local_address);
            m.push(Protocol::Tcp(self.tcp_port));
            m
        };
        let peer_id = self.local_peer_id;

        tracing::info!(
            "The p2p service starts on the `{listen_multiaddr}` with `{peer_id}`"
        );

        // start listening at the given address
        self.swarm.listen_on(listen_multiaddr)?;
        Ok(())
    }

    #[cfg(feature = "test-helpers")]
    pub fn listeners(&self) -> impl Iterator<Item = &Multiaddr> {
        self.swarm.listeners()
    }

    pub fn get_peers_ids(&self) -> impl Iterator<Item = &PeerId> {
        self.swarm.behaviour().get_peers_ids()
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
                let peers = self.get_peers_ids();
                let peers_count = self.swarm.behaviour().total_peers_connected();

                if peers_count == 0 {
                    return Err(RequestError::NoPeersConnected)
                }

                let mut range = rand::thread_rng();
                *peers.choose(&mut range).unwrap()
            }
        };

        let request_id = self
            .swarm
            .behaviour_mut()
            .send_request_msg(message_request, &peer_id);

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
            self.network_codec.convert_to_network_response(&message),
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

    pub fn report_message_validation_result(
        &mut self,
        msg_id: &MessageId,
        propagation_source: &PeerId,
        acceptance: MessageAcceptance,
    ) -> Result<bool, PublishError> {
        self.swarm.behaviour_mut().report_message_validation_result(
            msg_id,
            propagation_source,
            acceptance,
        )
    }

    pub fn update_block_height(&mut self, block_height: BlockHeight) {
        self.swarm.behaviour_mut().update_block_height(block_height)
    }

    #[tracing::instrument(skip_all,
        level = "debug",
        fields(
            local_peer_id = %self.local_peer_id,
            local_address = %self.local_address
        ),
        ret
    )]
    /// Handles P2P Events.
    /// Returns only events that are of interest to the Network Orchestrator.
    pub async fn next_event(&mut self) -> Option<FuelP2PEvent> {
        // TODO: add handling for when the stream closes and return None only when there are no
        //       more events to consume
        let event = self.swarm.select_next_some().await;
        tracing::debug!(?event);
        match event {
            SwarmEvent::Behaviour(fuel_behaviour) => {
                self.handle_behaviour_event(fuel_behaviour)
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                tracing::info!("Listening for p2p traffic on `{address}`");
                None
            }
            SwarmEvent::ListenerClosed {
                addresses, reason, ..
            } => {
                tracing::info!(
                    "p2p listener(s) `{addresses:?}` closed with `{reason:?}`"
                );
                None
            }
            _ => None,
        }
    }

    pub fn peer_manager(&self) -> &PeerManagerBehaviour {
        self.swarm.behaviour().peer_manager()
    }

    fn handle_behaviour_event(
        &mut self,
        event: FuelBehaviourEvent,
    ) -> Option<FuelP2PEvent> {
        match event {
            FuelBehaviourEvent::Discovery(discovery_event) => {
                if let DiscoveryEvent::PeerInfoOnConnect { peer_id, addresses } =
                    discovery_event
                {
                    self.swarm
                        .behaviour_mut()
                        .add_addresses_to_peer_info(&peer_id, addresses);
                }
            }
            FuelBehaviourEvent::Gossipsub(gossipsub_event) => {
                if let GossipsubEvent::Message {
                    propagation_source,
                    message,
                    message_id,
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
                                    message_id,
                                    topic_hash: message.topic,
                                    message: decoded_message,
                                })
                            }
                            Err(err) => {
                                warn!(target: "fuel-libp2p", "Failed to decode a message. ID: {}, Message: {:?} with error: {:?}", message_id, &message.data, err);

                                match self.report_message_validation_result(
                                    &message_id,
                                    &propagation_source,
                                    MessageAcceptance::Reject,
                                ) {
                                    Ok(false) => {
                                        warn!(target: "fuel-libp2p", "Message was not found in the cache, peer with PeerId: {} has been reported.", propagation_source);
                                    }
                                    Ok(true) => {
                                        info!(target: "fuel-libp2p", "Message found in the cache, peer with PeerId: {} has been reported.", propagation_source);
                                    }
                                    Err(e) => {
                                        warn!(target: "fuel-libp2p", "Failed to publish the message with following error: {:?}.", e);
                                    }
                                }
                            }
                        }
                    } else {
                        warn!(target: "fuel-libp2p", "GossipTopicTag does not exist for {:?}", &message.topic);
                    }
                }
            }

            FuelBehaviourEvent::PeerInfo(peer_info_event) => match peer_info_event {
                PeerInfoEvent::PeerIdentified { peer_id, addresses } => {
                    if self.metrics {
                        P2P_METRICS.unique_peers.inc();
                    }
                    self.swarm
                        .behaviour_mut()
                        .add_addresses_to_discovery(&peer_id, addresses);
                }
                PeerInfoEvent::PeerInfoUpdated {
                    peer_id,
                    block_height,
                } => {
                    return Some(FuelP2PEvent::PeerInfoUpdated {
                        peer_id,
                        block_height,
                    })
                }
                PeerInfoEvent::PeerConnected(peer_id) => {
                    return Some(FuelP2PEvent::PeerConnected(peer_id))
                }
                PeerInfoEvent::ReconnectToPeer(peer_id) => {
                    let _ = self.swarm.dial(peer_id);
                }
                PeerInfoEvent::PeerDisconnected {
                    peer_id,
                    should_reconnect,
                } => {
                    if should_reconnect {
                        let _ = self.swarm.dial(peer_id);
                    }
                    return Some(FuelP2PEvent::PeerDisconnected(peer_id))
                }
                PeerInfoEvent::TooManyPeers { peer_to_disconnect } => {
                    // disconnect the surplus peer
                    let _ = self.swarm.disconnect_peer_id(peer_to_disconnect);
                }
            },
            FuelBehaviourEvent::RequestResponse(req_res_event) => match req_res_event {
                RequestResponseEvent::Message { peer, message } => match message {
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
                                Some(ResponseChannelItem::Block(channel)),
                                Ok(ResponseMessage::SealedBlock(block)),
                            ) => {
                                if channel.send(block).is_err() {
                                    debug!(
                                        "Failed to send through the channel for {:?}",
                                        request_id
                                    );
                                }
                            }
                            (
                                Some(ResponseChannelItem::Transactions(channel)),
                                Ok(ResponseMessage::Transactions(transactions)),
                            ) => {
                                if channel.send(transactions).is_err() {
                                    debug!(
                                        "Failed to send through the channel for {:?}",
                                        request_id
                                    );
                                }
                            }
                            (
                                Some(ResponseChannelItem::SealedHeader(channel)),
                                Ok(ResponseMessage::SealedHeader(header)),
                            ) => {
                                if channel.send(header.map(|h| (peer, h))).is_err() {
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
        codecs::postcard::PostcardCodec,
        config::Config,
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
        p2p_service::FuelP2PEvent,
        peer_manager::PeerInfo,
        request_response::messages::{
            OutboundResponse,
            RequestMessage,
            ResponseChannelItem,
        },
        service::to_message_acceptance,
    };
    use fuel_core_types::{
        blockchain::{
            block::Block,
            consensus::{
                poa::PoAConsensus,
                Consensus,
                ConsensusVote,
            },
            header::PartialBlockHeader,
            primitives::BlockId,
            SealedBlock,
            SealedBlockHeader,
        },
        fuel_tx::Transaction,
        services::p2p::GossipsubMessageAcceptance,
    };
    use futures::StreamExt;
    use libp2p::{
        gossipsub::{
            error::PublishError,
            Topic,
        },
        identity::Keypair,
        multiaddr::Protocol,
        swarm::SwarmEvent,
        Multiaddr,
        PeerId,
    };
    use std::{
        collections::HashSet,
        net::{
            IpAddr,
            Ipv4Addr,
            SocketAddrV4,
            TcpListener,
        },
        sync::Arc,
        time::Duration,
    };
    use tokio::sync::{
        mpsc,
        oneshot,
        watch,
    };
    use tracing_attributes::instrument;

    /// helper function for building FuelP2PService
    fn build_service_from_config(
        mut p2p_config: Config,
    ) -> FuelP2PService<PostcardCodec> {
        p2p_config.keypair = Keypair::generate_secp256k1(); // change keypair for each Node
        let max_block_size = p2p_config.max_block_size;

        let mut service =
            FuelP2PService::new(p2p_config, PostcardCodec::new(max_block_size));
        service.start().unwrap();
        service
    }

    /// returns a free tcp port number for a node to listen on
    fn get_unused_port() -> u16 {
        let socket = SocketAddrV4::new(Ipv4Addr::from([0, 0, 0, 0]), 0);

        TcpListener::bind(socket)
            .and_then(|listener| listener.local_addr())
            .map(|addr| addr.port())
            // Safety: used only in tests, it is expected that there exists a free port
            .expect("A free tcp port exists")
    }

    /// Holds node data needed to initialize the P2P Service
    /// It provides correct `multiaddr` for other nodes to connect to
    #[derive(Debug, Clone)]
    struct NodeData {
        keypair: Keypair,
        tcp_port: u16,
        multiaddr: Multiaddr,
    }

    impl NodeData {
        /// Generates a random `keypair` and takes a free tcp port and returns it as `NodeData`
        fn random() -> Self {
            let keypair = Keypair::generate_secp256k1();
            let tcp_port = get_unused_port();

            let multiaddr = {
                let mut addr =
                    Multiaddr::from(IpAddr::V4(Ipv4Addr::from([127, 0, 0, 1])));
                addr.push(Protocol::Tcp(tcp_port));
                let peer_id = PeerId::from_public_key(&keypair.public());
                format!("{addr}/p2p/{peer_id}").parse().unwrap()
            };

            Self {
                keypair,
                tcp_port,
                multiaddr,
            }
        }

        /// Combines `NodeData` with `P2pConfig` to create a `FuelP2PService`
        fn create_service(
            &self,
            mut p2p_config: Config,
        ) -> FuelP2PService<PostcardCodec> {
            let max_block_size = p2p_config.max_block_size;
            p2p_config.tcp_port = self.tcp_port;
            p2p_config.keypair = self.keypair.clone();

            let mut service =
                FuelP2PService::new(p2p_config, PostcardCodec::new(max_block_size));
            service.start().unwrap();
            service
        }
    }

    fn spawn(stop: &watch::Sender<()>, mut node: FuelP2PService<PostcardCodec>) {
        let mut stop = stop.subscribe();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = node.next_event() => {}
                    _ = stop.changed() => {
                        break;
                    }
                }
            }
        });
    }

    #[tokio::test]
    #[instrument]
    async fn p2p_service_works() {
        let mut fuel_p2p_service =
            build_service_from_config(Config::default_initialized("p2p_service_works"));

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

    // Single sentry node connects to multiple reserved nodes and `max_peers_allowed` amount of non-reserved nodes.
    // It also tries to dial extra non-reserved nodes to establish the connection.
    // A single reserved node is not started immediately with the rest of the nodes.
    // Once sentry node establishes the connection with the allowed number of nodes
    // we start the reserved node, and await for it to establish the connection.
    // This test proves that there is always an available slot for the reserved node to connect to.
    #[tokio::test(flavor = "multi_thread")]
    #[instrument]
    async fn reserved_nodes_reconnect_works() {
        let mut p2p_config =
            Config::default_initialized("reserved_nodes_reconnect_works");
        // enable mdns for faster discovery of nodes
        p2p_config.enable_mdns = true;

        // total amount will be `max_peers_allowed` + `reserved_nodes.len()`
        let max_peers_allowed = 3;

        let bootstrap_nodes_data: Vec<NodeData> = (0..max_peers_allowed * 5)
            .map(|_| NodeData::random())
            .collect();

        let mut reserved_nodes_data: Vec<NodeData> =
            (0..3).map(|_| NodeData::random()).collect();

        let mut sentry_node = {
            let mut p2p_config = p2p_config.clone();
            p2p_config.max_peers_connected = max_peers_allowed as u32;

            p2p_config.bootstrap_nodes = bootstrap_nodes_data
                .iter()
                .map(|node| node.multiaddr.clone())
                .collect();

            p2p_config.reserved_nodes = reserved_nodes_data
                .iter()
                .map(|node| node.multiaddr.clone())
                .collect();

            NodeData::random().create_service(p2p_config)
        };

        // pop() a single reserved node, so it's not run with the rest of the nodes
        let reserved_node = reserved_nodes_data.pop().unwrap().clone();
        let reserved_node_peer_id =
            PeerId::from_public_key(&reserved_node.keypair.public());

        let all_node_services: Vec<_> = bootstrap_nodes_data
            .iter()
            .chain(reserved_nodes_data.iter())
            .map(|node| node.create_service(p2p_config.clone()))
            .collect();

        let mut all_nodes_ids: Vec<PeerId> = all_node_services
            .iter()
            .map(|service| service.local_peer_id)
            .collect();

        let (stop_sender, _) = watch::channel(());
        all_node_services.into_iter().for_each(|node| {
            spawn(&stop_sender, node);
        });

        loop {
            tokio::select! {
                sentry_node_event = sentry_node.next_event() => {
                    // we've connected to all other peers
                    if sentry_node.swarm.behaviour().total_peers_connected() >= 5 {
                        // if the `reserved_node` is not included,
                        // create and insert it, to be polled with rest of the nodes
                        if !all_nodes_ids
                        .iter()
                        .any(|local_peer_id| local_peer_id == &reserved_node_peer_id) {
                            let node = reserved_node.create_service(p2p_config.clone());
                            all_nodes_ids.push(node.local_peer_id);
                            spawn(&stop_sender, node);
                        }
                    }
                    if let Some(FuelP2PEvent::PeerConnected(peer_id)) = sentry_node_event {
                        // we connected to the desired reserved node
                        if peer_id == reserved_node_peer_id {
                            break
                        }
                    }
                },
            }
        }
        stop_sender.send(()).unwrap();
    }

    // We start with two nodes, node_5 and node_10, bootstrapped with 100 other nodes
    // yet node_5 is only allowed to connect to 5 other nodes, and node_10 to 10
    #[tokio::test]
    #[instrument]
    async fn max_peers_connected_works() {
        let mut p2p_config = Config::default_initialized("max_peers_connected_works");
        // enable mdns for faster discovery of nodes
        p2p_config.enable_mdns = true;

        let nodes: Vec<NodeData> = (0..100).map(|_| NodeData::random()).collect();

        // this node is allowed to only connect to 5 other nodes
        let mut node_5 = {
            let mut p2p_config = p2p_config.clone();
            p2p_config.max_peers_connected = 5;
            // it still tries to dial all 100 nodes!
            p2p_config.bootstrap_nodes =
                nodes.iter().map(|node| node.multiaddr.clone()).collect();

            NodeData::random().create_service(p2p_config)
        };

        // this node is allowed to only connect to 10 other nodes
        let mut node_10 = {
            let mut p2p_config = p2p_config.clone();
            p2p_config.max_peers_connected = 10;
            // it still tries to dial all 100 nodes!
            p2p_config.bootstrap_nodes =
                nodes.iter().map(|node| node.multiaddr.clone()).collect();

            NodeData::random().create_service(p2p_config)
        };

        let mut node_services: Vec<_> = nodes
            .into_iter()
            .map(|node| node.create_service(p2p_config.clone()))
            .collect();

        // this node will only connect to node_5 and node_10 at the beginning
        // then it will slowly discover other nodes in the network
        // it serves as our exit from the loop
        let mut bootstrapped_node = node_services.pop().unwrap();

        let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();
        let jh = tokio::spawn(async move {
            while rx.try_recv().is_err() {
                futures::stream::iter(node_services.iter_mut())
                    .for_each_concurrent(20, |node| async move {
                        node.next_event().await;
                    })
                    .await;
            }
        });

        loop {
            tokio::select! {
                event_from_node_5 = node_5.next_event() => {
                    if let Some(FuelP2PEvent::PeerConnected(_)) = event_from_node_5 {
                        if node_5.swarm.connected_peers().count() > 5 {
                            panic!("The node should only connect to max 5 peers");
                        }
                    }
                    tracing::info!("Event from the node_5: {:?}", event_from_node_5);
                },
                event_from_node_10 = node_10.next_event() => {
                    if let Some(FuelP2PEvent::PeerConnected(_)) = event_from_node_10 {
                        if node_10.swarm.connected_peers().count() > 10 {
                            panic!("The node should only connect to max 10 peers");
                        }
                    }
                    tracing::info!("Event from the node_10: {:?}", event_from_node_10);
                },
                event_from_bootstrapped_node = bootstrapped_node.next_event() => {
                    if let Some(FuelP2PEvent::PeerConnected(_)) = event_from_bootstrapped_node {
                        // if the test was broken, it would panic! by the time this node discovers more peers
                        // and connects to them
                        if bootstrapped_node.swarm.connected_peers().count() > 20 {
                            break
                        }
                    }
                    tracing::info!("Event from the bootstrapped_node: {:?}", event_from_bootstrapped_node);
                },
            }
        }

        tx.send(()).unwrap();
        jh.await.unwrap()
    }

    // Simulate 2 Sets of Sentry nodes.
    // In both Sets, a single Guarded Node should only be connected to their sentry nodes.
    // While other nodes can and should connect to nodes outside of the Sentry Set.
    #[tokio::test(flavor = "multi_thread")]
    #[instrument]
    async fn sentry_nodes_working() {
        let reserved_nodes_size = 4;

        let mut p2p_config = Config::default_initialized("sentry_nodes_working");
        // enable mdns for faster discovery of nodes
        p2p_config.enable_mdns = true;

        let build_sentry_nodes = || {
            let guarded_node = NodeData::random();

            let reserved_nodes: Vec<NodeData> = (0..reserved_nodes_size)
                .map(|_| NodeData::random())
                .collect();

            // set up the guraded node service with `reserved_nodes_only_mode`
            let guarded_node_service = {
                let mut p2p_config = p2p_config.clone();
                p2p_config.reserved_nodes = reserved_nodes
                    .iter()
                    .map(|node| node.multiaddr.clone())
                    .collect();
                p2p_config.reserved_nodes_only_mode = true;
                guarded_node.create_service(p2p_config)
            };

            let sentry_nodes: Vec<FuelP2PService<_>> = reserved_nodes
                .into_iter()
                .map(|node| {
                    let mut p2p_config = p2p_config.clone();
                    // sentry nodes will hold a guarded node as their reserved node
                    p2p_config.reserved_nodes = vec![guarded_node.multiaddr.clone()];
                    node.create_service(p2p_config)
                })
                .collect();

            (guarded_node_service, sentry_nodes)
        };

        let (mut first_guarded_node, mut first_sentry_nodes) = build_sentry_nodes();
        let (mut second_guarded_node, second_sentry_nodes) = build_sentry_nodes();

        let mut first_sentry_set: HashSet<_> = first_sentry_nodes
            .iter()
            .map(|node| node.local_peer_id)
            .collect();

        let mut second_sentry_set: HashSet<_> = second_sentry_nodes
            .iter()
            .map(|node| node.local_peer_id)
            .collect();

        let mut single_sentry_node = first_sentry_nodes.pop().unwrap();
        let mut sentry_node_connections = HashSet::new();
        let (stop_sender, _) = watch::channel(());
        first_sentry_nodes
            .into_iter()
            .chain(second_sentry_nodes.into_iter())
            .for_each(|node| {
                spawn(&stop_sender, node);
            });

        loop {
            tokio::select! {
                event_from_first_guarded = first_guarded_node.next_event() => {
                    if let Some(FuelP2PEvent::PeerConnected(peer_id)) = event_from_first_guarded {
                        if !first_sentry_set.remove(&peer_id)            {
                            panic!("The node should only connect to the specified reserved nodes!");
                        }
                    }
                    tracing::info!("Event from the first guarded node: {:?}", event_from_first_guarded);
                },
                event_from_second_guarded = second_guarded_node.next_event() => {
                    if let Some(FuelP2PEvent::PeerConnected(peer_id)) = event_from_second_guarded {
                        if !second_sentry_set.remove(&peer_id)            {
                            panic!("The node should only connect to the specified reserved nodes!");
                        }
                    }
                    tracing::info!("Event from the second guarded node: {:?}", event_from_second_guarded);
                },
                // Poll one of the reserved, sentry nodes
                sentry_node_event = single_sentry_node.next_event() => {
                    if let Some(FuelP2PEvent::PeerConnected(peer_id)) = sentry_node_event {
                        sentry_node_connections.insert(peer_id);
                    }
                    // This reserved node has connected to more than the number of reserved nodes it is part of.
                    // It means it has discovered other nodes in the network.
                    if sentry_node_connections.len() > reserved_nodes_size {
                        // At the same time, the guarded nodes have only connected to their reserved nodes.
                        if first_sentry_set.is_empty() && first_sentry_set.is_empty() {
                            break;
                        }
                    }

                }
            };
        }
        stop_sender.send(()).unwrap();
    }

    // Simulates 2 p2p nodes that are on the same network and should connect via mDNS
    // without any additional bootstrapping
    #[tokio::test]
    #[instrument]
    async fn nodes_connected_via_mdns() {
        // Node A
        let mut p2p_config = Config::default_initialized("nodes_connected_via_mdns");
        p2p_config.enable_mdns = true;
        let mut node_a = build_service_from_config(p2p_config.clone());

        // Node B
        let mut node_b = build_service_from_config(p2p_config);

        loop {
            tokio::select! {
                node_b_event = node_b.next_event() => {
                    if let Some(FuelP2PEvent::PeerConnected(_)) = node_b_event {
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

    // Simulates 2 p2p nodes that are on the same network but their Fuel Upgrade checksum is different
    // (different chain id or chain config)
    // So they are not able to connect
    #[tokio::test]
    #[instrument]
    async fn nodes_cannot_connect_due_to_different_checksum() {
        use libp2p::{
            swarm::DialError,
            TransportError,
        };
        // Node A
        let mut p2p_config =
            Config::default_initialized("nodes_cannot_connect_due_to_different_checksum");
        p2p_config.enable_mdns = true;
        let mut node_a = build_service_from_config(p2p_config.clone());

        // different checksum
        p2p_config.checksum = [1u8; 32].into();
        // Node B
        let mut node_b = build_service_from_config(p2p_config);

        loop {
            tokio::select! {
                node_a_event = node_a.swarm.select_next_some() => {
                    tracing::info!("Node A Event: {:?}", node_a_event);
                    if let SwarmEvent::OutgoingConnectionError { peer_id: _, error: DialError::Transport(mut errors) } = node_a_event {
                        if let TransportError::Other(_) = errors.pop().unwrap().1 {
                            // Custom error confirmed
                            break
                        }
                    }
                },
                node_b_event = node_b.next_event() => {
                    if let Some(FuelP2PEvent::PeerConnected(_)) = node_b_event {
                        panic!("Node B should not connect to Node A!")
                    }
                    tracing::info!("Node B Event: {:?}", node_b_event);
                },

            };
        }
    }

    // Simulates 3 p2p nodes, Node B & Node C are bootstrapped with Node A
    // Using Identify Protocol Node C should be able to identify and connect to Node B
    #[tokio::test]
    #[instrument]
    async fn nodes_connected_via_identify() {
        // Node A
        let mut p2p_config = Config::default_initialized("nodes_connected_via_identify");

        let node_a_data = NodeData::random();
        let mut node_a = node_a_data.create_service(p2p_config.clone());

        // Node B
        p2p_config.bootstrap_nodes = vec![node_a_data.multiaddr];
        let mut node_b = build_service_from_config(p2p_config.clone());

        // Node C
        let mut node_c = build_service_from_config(p2p_config);

        loop {
            tokio::select! {
                node_a_event = node_a.next_event() => {
                    tracing::info!("Node A Event: {:?}", node_a_event);
                },
                node_b_event = node_b.next_event() => {
                    tracing::info!("Node B Event: {:?}", node_b_event);
                },

                node_c_event = node_c.next_event() => {
                    if let Some(FuelP2PEvent::PeerConnected(peer_id)) = node_c_event {
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
    // On sucessful connection, node B updates its latest BlockHeight
    // and shares it with Peer A via Heartbeat protocol
    #[tokio::test]
    #[instrument]
    async fn peer_info_updates_work() {
        let mut p2p_config = Config::default_initialized("peer_info_updates_work");

        // Node A
        let node_a_data = NodeData::random();
        let mut node_a = node_a_data.create_service(p2p_config.clone());

        // Node B
        p2p_config.bootstrap_nodes = vec![node_a_data.multiaddr];
        let mut node_b = build_service_from_config(p2p_config);

        let latest_block_height = 40_u32.into();

        loop {
            tokio::select! {
                node_a_event = node_a.next_event() => {
                    if let Some(FuelP2PEvent::PeerInfoUpdated { peer_id, block_height: _ }) = node_a_event {
                        if let Some(PeerInfo { peer_addresses, heartbeat_data, client_version, .. }) = node_a.swarm.behaviour().get_peer_info(&peer_id) {
                            // Exits after it verifies that:
                            // 1. Peer Addresses are known
                            // 2. Client Version is known
                            // 3. Node has responded with their latest BlockHeight
                            if !peer_addresses.is_empty() && client_version.is_some() && heartbeat_data.block_height == Some(latest_block_height) {
                                break;
                            }
                        }
                    }

                    tracing::info!("Node A Event: {:?}", node_a_event);
                },
                node_b_event = node_b.next_event() => {
                    if let Some(FuelP2PEvent::PeerConnected(_)) = node_b_event {
                        // we've connected to Peer A
                        // let's update our BlockHeight
                        node_b.update_block_height(latest_block_height);
                    }
                    tracing::info!("Node B Event: {:?}", node_b_event);
                }
            };
        }
    }

    #[tokio::test]
    #[instrument]
    async fn gossipsub_broadcast_tx_with_accept() {
        gossipsub_broadcast(
            GossipsubBroadcastRequest::NewTx(Arc::new(Transaction::default())),
            GossipsubMessageAcceptance::Accept,
        )
        .await;
    }

    #[tokio::test]
    #[instrument]
    async fn gossipsub_broadcast_tx_with_reject() {
        gossipsub_broadcast(
            GossipsubBroadcastRequest::NewTx(Arc::new(Transaction::default())),
            GossipsubMessageAcceptance::Reject,
        )
        .await;
    }

    #[tokio::test]
    #[instrument]
    async fn gossipsub_broadcast_vote_with_accept() {
        gossipsub_broadcast(
            GossipsubBroadcastRequest::ConsensusVote(Arc::new(ConsensusVote::default())),
            GossipsubMessageAcceptance::Accept,
        )
        .await;
    }

    #[tokio::test]
    #[instrument]
    async fn gossipsub_broadcast_vote_with_reject() {
        gossipsub_broadcast(
            GossipsubBroadcastRequest::ConsensusVote(Arc::new(ConsensusVote::default())),
            GossipsubMessageAcceptance::Reject,
        )
        .await;
    }

    #[tokio::test]
    #[instrument]
    async fn gossipsub_broadcast_block_with_accept() {
        gossipsub_broadcast(
            GossipsubBroadcastRequest::NewBlock(Arc::new(Block::default())),
            GossipsubMessageAcceptance::Accept,
        )
        .await;
    }

    #[tokio::test]
    #[instrument]
    async fn gossipsub_broadcast_block_with_ignore() {
        gossipsub_broadcast(
            GossipsubBroadcastRequest::NewBlock(Arc::new(Block::default())),
            GossipsubMessageAcceptance::Ignore,
        )
        .await;
    }

    /// Reusable helper function for Broadcasting Gossipsub requests
    async fn gossipsub_broadcast(
        broadcast_request: GossipsubBroadcastRequest,
        acceptance: GossipsubMessageAcceptance,
    ) {
        let mut p2p_config = Config::default_initialized("gossipsub_exchanges_messages");

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
        let node_a_data = NodeData::random();
        let mut node_a = node_a_data.create_service(p2p_config.clone());

        // Node B
        let node_b_data = NodeData::random();
        p2p_config.bootstrap_nodes = vec![node_a_data.multiaddr];
        let mut node_b = node_b_data.create_service(p2p_config.clone());

        // Node C
        p2p_config.bootstrap_nodes = vec![node_b_data.multiaddr];
        let mut node_c = build_service_from_config(p2p_config.clone());

        // Node C does not connecto to Node A
        // it should receive the propagated message from Node B if `GossipsubMessageAcceptance` is `Accept`
        node_c.swarm.ban_peer_id(node_a.local_peer_id);

        loop {
            tokio::select! {
                node_a_event = node_a.next_event() => {
                    if let Some(FuelP2PEvent::PeerInfoUpdated { peer_id, block_height: _ }) = node_a_event {
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
                    if let Some(FuelP2PEvent::GossipsubMessage { topic_hash, message, message_id, peer_id }) = node_b_event.clone() {
                        // Message Validation must be reported
                        // If it's `Accept`, Node B will propagate the message to Node C
                        // If it's `Ignore` or `Reject`, Node C should not receive anything
                        let msg_acceptance = to_message_acceptance(&acceptance);
                        let _ = node_b.report_message_validation_result(&message_id, &peer_id, msg_acceptance);
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
                                if block.header().height() != Block::<Transaction>::default().header().height() {
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

                        // Node B received the correct message
                        // If we try to publish it again we will get `PublishError::Duplicate`
                        // This asserts that our MessageId calculation is consistant irrespective of which Peer sends it
                        let broadcast_request = broadcast_request.clone();
                        matches!(node_b.publish_message(broadcast_request), Err(PublishError::Duplicate));

                        match acceptance {
                            GossipsubMessageAcceptance::Reject | GossipsubMessageAcceptance::Ignore => {
                                break
                            },
                            _ => {
                                // the `exit` should happen in Node C
                            }
                        }
                    }

                    tracing::info!("Node B Event: {:?}", node_b_event);
                }

                node_c_event = node_c.next_event() => {
                    if let Some(FuelP2PEvent::GossipsubMessage { peer_id, .. }) = node_c_event.clone() {
                        // Node B should be the source propagator
                        assert!(peer_id == node_b.local_peer_id);
                        match acceptance {
                            GossipsubMessageAcceptance::Reject | GossipsubMessageAcceptance::Ignore => {
                                panic!("Node C should not receive Rejected or Ignored messages")
                            },
                            GossipsubMessageAcceptance::Accept => {
                                break
                            }
                        }
                    }
                }
            };
        }
    }

    async fn request_response_works_with(request_msg: RequestMessage) {
        let mut p2p_config = Config::default_initialized("request_response_works_with");

        // Node A
        let node_a_data = NodeData::random();
        let mut node_a = node_a_data.create_service(p2p_config.clone());

        // Node B
        p2p_config.bootstrap_nodes = vec![node_a_data.multiaddr];
        let mut node_b = build_service_from_config(p2p_config.clone());

        let (tx_test_end, mut rx_test_end) = mpsc::channel(1);

        let mut request_sent = false;

        loop {
            tokio::select! {
                message_sent = rx_test_end.recv() => {
                    // we received a signal to end the test
                    assert_eq!(message_sent, Some(true), "Receuved incorrect or missing missing messsage");
                    break;
                }
                node_a_event = node_a.next_event() => {
                    if let Some(FuelP2PEvent::PeerInfoUpdated { peer_id, block_height: _ }) = node_a_event {
                        if let Some(PeerInfo { peer_addresses, .. }) = node_a.swarm.behaviour().get_peer_info(&peer_id) {
                            // 0. verifies that we've got at least a single peer address to request message from
                            if !peer_addresses.is_empty() && !request_sent {
                                request_sent = true;

                                match request_msg {
                                    RequestMessage::Block(_) => {
                                        let (tx_orchestrator, rx_orchestrator) = oneshot::channel();
                                        assert!(node_a.send_request_msg(None, request_msg, ResponseChannelItem::Block(tx_orchestrator)).is_ok());
                                        let tx_test_end = tx_test_end.clone();

                                        tokio::spawn(async move {
                                            let response_message = rx_orchestrator.await;

                                            if let Ok(Some(sealed_block)) = response_message {
                                                let _ = tx_test_end.send(*sealed_block.entity.header().height() == 0_u64.into()).await;
                                            } else {
                                                tracing::error!("Orchestrator failed to receive a message: {:?}", response_message);
                                                let _ = tx_test_end.send(false).await;
                                            }
                                        });

                                    }
                                    RequestMessage::SealedHeader(_) => {
                                        let (tx_orchestrator, rx_orchestrator) = oneshot::channel();
                                        assert!(node_a.send_request_msg(None, request_msg, ResponseChannelItem::SealedHeader(tx_orchestrator)).is_ok());
                                        let tx_test_end = tx_test_end.clone();

                                        tokio::spawn(async move {
                                            let response_message = rx_orchestrator.await;

                                            if let Ok(Some(_)) = response_message {
                                                let _ = tx_test_end.send(true).await;
                                            } else {
                                                tracing::error!("Orchestrator failed to receive a message: {:?}", response_message);
                                                let _ = tx_test_end.send(false).await;
                                            }
                                        });
                                    }
                                    RequestMessage::Transactions(_) => {
                                        let (tx_orchestrator, rx_orchestrator) = oneshot::channel();
                                        assert!(node_a.send_request_msg(None, request_msg, ResponseChannelItem::Transactions(tx_orchestrator)).is_ok());
                                        let tx_test_end = tx_test_end.clone();

                                        tokio::spawn(async move {
                                            let response_message = rx_orchestrator.await;

                                            if let Ok(Some(transactions)) = response_message {
                                                let _ = tx_test_end.send(transactions.len() == 5).await;
                                            } else {
                                                tracing::error!("Orchestrator failed to receive a message: {:?}", response_message);
                                                let _ = tx_test_end.send(false).await;
                                            }
                                        });
                                    }
                                }
                            }
                        }
                    }

                    tracing::info!("Node A Event: {:?}", node_a_event);
                },
                node_b_event = node_b.next_event() => {
                    // 2. Node B receives the RequestMessage from Node A initiated by the NetworkOrchestrator
                    if let Some(FuelP2PEvent::RequestMessage{ request_id, request_message: received_request_message }) = node_b_event {
                        match received_request_message {
                            RequestMessage::Block(_) => {
                                let block = Block::new(PartialBlockHeader::default(), vec![Transaction::default(), Transaction::default(), Transaction::default(), Transaction::default(), Transaction::default()], &[]);

                                let sealed_block = SealedBlock {
                                    entity: block,
                                    consensus: Consensus::PoA(PoAConsensus::new(Default::default())),
                                };

                                let _ = node_b.send_response_msg(request_id, OutboundResponse::Block(Some(Arc::new(sealed_block))));
                            }
                            RequestMessage::SealedHeader(_) => {
                                let header = Default::default();

                                let sealed_header = SealedBlockHeader {
                                    entity: header,
                                    consensus: Consensus::PoA(PoAConsensus::new(Default::default())),
                                };

                                let _ = node_b.send_response_msg(request_id, OutboundResponse::SealedHeader(Some(Arc::new(sealed_header))));
                            }
                            RequestMessage::Transactions(_) => {
                                let transactions = vec![Transaction::default(), Transaction::default(), Transaction::default(), Transaction::default(), Transaction::default()];
                                let _ = node_b.send_response_msg(request_id, OutboundResponse::Transactions(Some(Arc::new(transactions))));
                            }
                        }

                    }

                    tracing::info!("Node B Event: {:?}", node_b_event);
                }
            };
        }
    }

    #[tokio::test]
    #[instrument]
    async fn request_response_works_with_transactions() {
        request_response_works_with(RequestMessage::Transactions(BlockId::default()))
            .await
    }

    #[tokio::test]
    #[instrument]
    async fn request_response_works_with_block() {
        request_response_works_with(RequestMessage::Block(0_u64.into())).await
    }

    #[tokio::test]
    #[instrument]
    async fn request_response_works_with_sealed_header() {
        request_response_works_with(RequestMessage::SealedHeader(0_u64.into())).await
    }

    #[tokio::test]
    #[instrument]
    async fn req_res_outbound_timeout_works() {
        let mut p2p_config =
            Config::default_initialized("req_res_outbound_timeout_works");

        // Node A
        // setup request timeout to 0 in order for the Request to fail
        p2p_config.set_request_timeout = Duration::from_secs(0);

        let node_a_data = NodeData::random();
        let mut node_a = node_a_data.create_service(p2p_config.clone());

        // Node B
        p2p_config.bootstrap_nodes = vec![node_a_data.multiaddr];
        let mut node_b = build_service_from_config(p2p_config.clone());

        let (tx_test_end, mut rx_test_end) = tokio::sync::mpsc::channel(1);

        // track the request sent in order to avoid duplicate sending
        let mut request_sent = false;

        loop {
            tokio::select! {
                node_a_event = node_a.next_event() => {
                    if let Some(FuelP2PEvent::PeerInfoUpdated { peer_id, block_height: _ }) = node_a_event {
                        if let Some(PeerInfo { peer_addresses, .. }) = node_a.swarm.behaviour().get_peer_info(&peer_id) {
                            // 0. verifies that we've got at least a single peer address to request message from
                            if !peer_addresses.is_empty() && !request_sent {
                                request_sent = true;

                                // 1. Simulating Oneshot channel from the NetworkOrchestrator
                                let (tx_orchestrator, rx_orchestrator) = oneshot::channel();

                                // 2a. there should be ZERO pending outbound requests in the table
                                assert_eq!(node_a.outbound_requests_table.len(), 0);

                                // Request successfully sent
                                let requested_block_height = RequestMessage::Block(0_u64.into());
                                assert!(node_a.send_request_msg(None, requested_block_height, ResponseChannelItem::Block(tx_orchestrator)).is_ok());

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
