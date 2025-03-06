use crate::{
    behavior::{
        FuelBehaviour,
        FuelBehaviourEvent,
    },
    codecs::{
        gossipsub::GossipsubMessageHandler,
        postcard::PostcardCodec,
        request_response::RequestResponseMessageHandler,
        GossipsubCodec,
    },
    config::{
        build_transport_function,
        Config,
    },
    dnsaddr_resolution::DnsResolver,
    gossipsub::{
        messages::{
            GossipTopicTag,
            GossipsubBroadcastRequest,
            GossipsubMessage as FuelGossipsubMessage,
        },
        topics::GossipsubTopics,
    },
    heartbeat,
    peer_manager::{
        ConnectionState,
        PeerManager,
        Punisher,
    },
    peer_report::PeerReportEvent,
    request_response::messages::{
        RequestError,
        RequestMessage,
        ResponseError,
        ResponseSendError,
        ResponseSender,
        V2ResponseMessage,
    },
    TryPeerId,
};
use fuel_core_metrics::{
    global_registry,
    p2p_metrics::increment_unique_peers,
};
use fuel_core_types::{
    fuel_types::BlockHeight,
    services::p2p::peer_reputation::AppScore,
};
use futures::prelude::*;
use libp2p::{
    gossipsub::{
        self,
        MessageAcceptance,
        MessageId,
        PublishError,
        TopicHash,
    },
    identify,
    metrics::{
        Metrics,
        Recorder,
    },
    multiaddr::Protocol,
    request_response::{
        self,
        InboundRequestId,
        OutboundFailure,
        OutboundRequestId,
        ResponseChannel,
    },
    swarm::SwarmEvent,
    tcp,
    Multiaddr,
    PeerId,
    Swarm,
    SwarmBuilder,
};
use rand::seq::IteratorRandom;
use std::{
    collections::HashMap,
    time::Duration,
};
use tokio::sync::broadcast;
use tracing::{
    debug,
    warn,
};

#[cfg(test)]
mod tests;

/// Maximum amount of peer's addresses that we are ready to store per peer
const MAX_IDENTIFY_ADDRESSES: usize = 10;

impl Punisher for Swarm<FuelBehaviour> {
    fn ban_peer(&mut self, peer_id: PeerId) {
        self.behaviour_mut().block_peer(peer_id)
    }
}

/// Listens to the events on the p2p network
/// And forwards them to the Orchestrator
pub struct FuelP2PService {
    /// Store the local peer id
    pub local_peer_id: PeerId,

    /// IP address for Swarm to listen on
    local_address: std::net::IpAddr,

    /// The TCP port that Swarm listens on
    tcp_port: u16,

    /// Swarm handler for FuelBehaviour
    swarm: Swarm<FuelBehaviour>,

    /// Holds active outbound requests and associated oneshot channels.
    /// When we send a request to the p2p network, we add it here. The sender
    /// must provide a channel to receive the response.
    /// Whenever a response (or an error) is received from the p2p network,
    /// the request is removed from this table, and the channel is used to
    /// send the result to the caller.
    outbound_requests_table: HashMap<OutboundRequestId, ResponseSender>,

    /// Holds active inbound requests and associated oneshot channels.
    /// Whenever we're done processing the request, it's removed from this table,
    /// and the channel is used to send the result to libp2p, which will forward it
    /// to the peer that requested it.
    inbound_requests_table: HashMap<InboundRequestId, ResponseChannel<V2ResponseMessage>>,

    /// `PostcardCodec` as GossipsubCodec for encoding and decoding of Gossipsub messages
    gossipsub_codec: GossipsubMessageHandler<PostcardCodec>,

    /// Stores additional p2p network info
    network_metadata: NetworkMetadata,

    /// Whether or not metrics collection is enabled
    metrics: bool,

    /// libp2p metrics registry
    libp2p_metrics_registry: Option<Metrics>,

    /// Holds peers' information, and manages existing connections
    peer_manager: PeerManager,
}

#[derive(Debug)]
struct GossipsubData {
    topics: GossipsubTopics,
}

impl GossipsubData {
    pub fn with_topics(topics: GossipsubTopics) -> Self {
        Self { topics }
    }
}

/// Holds additional Network data for FuelBehavior
#[derive(Debug)]
struct NetworkMetadata {
    gossipsub_data: GossipsubData,
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
    NewSubscription {
        peer_id: PeerId,
        tag: GossipTopicTag,
    },
    InboundRequestMessage {
        request_id: InboundRequestId,
        request_message: RequestMessage,
    },
    PeerConnected(PeerId),
    PeerDisconnected(PeerId),
    PeerInfoUpdated {
        peer_id: PeerId,
        block_height: BlockHeight,
    },
}

async fn parse_multiaddrs(multiaddrs: Vec<Multiaddr>) -> anyhow::Result<Vec<Multiaddr>> {
    let dnsaddr_urls = multiaddrs
        .iter()
        .filter_map(|multiaddr| {
            if let Protocol::Dnsaddr(dnsaddr_url) = multiaddr.iter().next()? {
                Some(dnsaddr_url.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let dns_resolver = DnsResolver::new().await?;
    let mut dnsaddr_multiaddrs = vec![];

    for dnsaddr in dnsaddr_urls {
        let multiaddrs = dns_resolver.lookup_dnsaddr(dnsaddr.as_ref()).await?;
        dnsaddr_multiaddrs.extend(multiaddrs);
    }

    let resolved_multiaddrs = multiaddrs
        .into_iter()
        .filter(|multiaddr| !multiaddr.iter().any(|p| matches!(p, Protocol::Dnsaddr(_))))
        .chain(dnsaddr_multiaddrs.into_iter())
        .collect();
    Ok(resolved_multiaddrs)
}

impl FuelP2PService {
    pub async fn new(
        reserved_peers_updates: broadcast::Sender<usize>,
        config: Config,
        gossipsub_codec: GossipsubMessageHandler<PostcardCodec>,
        request_response_codec: RequestResponseMessageHandler<PostcardCodec>,
    ) -> anyhow::Result<Self> {
        let metrics = config.metrics;

        let gossipsub_data =
            GossipsubData::with_topics(GossipsubTopics::new(&config.network_name));
        let network_metadata = NetworkMetadata { gossipsub_data };

        let mut config = config;
        // override the configuration with the parsed multiaddrs from dnsaddr resolution
        config.bootstrap_nodes = parse_multiaddrs(config.bootstrap_nodes).await?;
        config.reserved_nodes = parse_multiaddrs(config.reserved_nodes).await?;

        // configure and build P2P Service
        let (connection_state_writer, connection_state_reader) = ConnectionState::new();
        let transport_function =
            build_transport_function(&config, connection_state_reader);
        let tcp_config = tcp::Config::new();

        let behaviour = FuelBehaviour::new(&config, request_response_codec)?;

        let swarm_builder = SwarmBuilder::with_existing_identity(config.keypair.clone())
            .with_tokio()
            .with_tcp(
                tcp_config,
                transport_function,
                libp2p::yamux::Config::default,
            )
            .map_err(|_| anyhow::anyhow!("Failed to build Swarm"))?
            .with_dns()?;

        let mut libp2p_metrics_registry = None;
        let mut swarm = if metrics {
            // we use the global registry to store the metrics without needing to create a new one
            // since libp2p already creates sub-registries
            let mut registry = global_registry().registry.lock();
            libp2p_metrics_registry = Some(Metrics::new(&mut registry));

            swarm_builder
                .with_bandwidth_metrics(&mut registry)
                .with_behaviour(|_| behaviour)?
                .with_swarm_config(|cfg| {
                    if let Some(timeout) = config.connection_idle_timeout {
                        cfg.with_idle_connection_timeout(timeout)
                    } else {
                        cfg
                    }
                })
                .build()
        } else {
            swarm_builder
                .with_behaviour(|_| behaviour)?
                .with_swarm_config(|cfg| {
                    if let Some(timeout) = config.connection_idle_timeout {
                        cfg.with_idle_connection_timeout(timeout)
                    } else {
                        cfg
                    }
                })
                .build()
        };

        let local_peer_id = swarm.local_peer_id().to_owned();

        if let Some(public_address) = config.public_address.clone() {
            swarm.add_external_address(public_address);
        }

        let reserved_peers = config
            .reserved_nodes
            .iter()
            .filter_map(|m| m.try_to_peer_id())
            .collect();

        Ok(Self {
            local_peer_id,
            local_address: config.address,
            tcp_port: config.tcp_port,
            swarm,
            gossipsub_codec,
            outbound_requests_table: HashMap::default(),
            inbound_requests_table: HashMap::default(),
            network_metadata,
            metrics,
            libp2p_metrics_registry,
            peer_manager: PeerManager::new(
                reserved_peers_updates,
                reserved_peers,
                connection_state_writer,
                usize::try_from(config.max_discovery_peers_connected)?,
            ),
        })
    }

    pub async fn start(&mut self) -> anyhow::Result<()> {
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

        // Wait for listener addresses.
        tokio::time::timeout(Duration::from_secs(5), self.await_listeners_address())
            .await
            .map_err(|_| {
                anyhow::anyhow!("P2PService should get a new address within 5 seconds")
            })?;
        Ok(())
    }

    async fn await_listeners_address(&mut self) {
        loop {
            if let SwarmEvent::NewListenAddr { .. } = self.swarm.select_next_some().await
            {
                break;
            }
        }
    }

    pub fn update_metrics<T>(&self, update_fn: T)
    where
        T: FnOnce(),
    {
        if self.metrics {
            update_fn();
        }
    }

    pub fn update_libp2p_metrics<E>(&self, event: &E)
    where
        Metrics: Recorder<E>,
    {
        if let Some(registry) = self.libp2p_metrics_registry.as_ref() {
            self.update_metrics(|| registry.record(event));
        }
    }

    #[cfg(feature = "test-helpers")]
    pub fn multiaddrs(&self) -> Vec<Multiaddr> {
        let local_peer = self.local_peer_id;
        self.swarm
            .listeners()
            .map(|addr| {
                format!("{addr}/p2p/{local_peer}")
                    .parse()
                    .expect("The format is always valid")
            })
            .collect()
    }

    pub fn get_peers_ids_iter(&self) -> impl Iterator<Item = &PeerId> {
        self.peer_manager.get_peers_ids()
    }

    pub fn publish_message(
        &mut self,
        message: GossipsubBroadcastRequest,
    ) -> Result<MessageId, PublishError> {
        let topic_hash = self
            .network_metadata
            .gossipsub_data
            .topics
            .get_gossipsub_topic_hash(&message);

        match self.gossipsub_codec.encode(message) {
            Ok(encoded_data) => self
                .swarm
                .behaviour_mut()
                .publish_message(topic_hash, encoded_data),
            Err(e) => Err(PublishError::TransformFailed(e)),
        }
    }

    /// Sends RequestMessage to a peer
    /// If the peer is not defined it will pick one at random
    /// Only returns error if no peers are connected
    pub fn send_request_msg(
        &mut self,
        peer_id: Option<PeerId>,
        message_request: RequestMessage,
        on_response: ResponseSender,
    ) -> Result<OutboundRequestId, RequestError> {
        let peer_id = match peer_id {
            Some(peer_id) => peer_id,
            _ => {
                let peers = self.get_peers_ids_iter();
                let peers_count = self.peer_manager.total_peers_connected();

                if peers_count == 0 {
                    return Err(RequestError::NoPeersConnected);
                }

                let mut range = rand::thread_rng();
                *peers.choose(&mut range).unwrap()
            }
        };

        let request_id = self
            .swarm
            .behaviour_mut()
            .send_request_msg(message_request, &peer_id);

        self.outbound_requests_table.insert(request_id, on_response);

        Ok(request_id)
    }

    /// Sends ResponseMessage to a peer that requested the data
    pub fn send_response_msg(
        &mut self,
        request_id: InboundRequestId,
        message: V2ResponseMessage,
    ) -> Result<(), ResponseSendError> {
        let Some(channel) = self.inbound_requests_table.remove(&request_id) else {
            debug!("ResponseChannel for {:?} does not exist!", request_id);
            return Err(ResponseSendError::ResponseChannelDoesNotExist);
        };

        if self
            .swarm
            .behaviour_mut()
            .send_response_msg(channel, message)
            .is_err()
        {
            debug!("Failed to send ResponseMessage for {:?}", request_id);
            return Err(ResponseSendError::SendingResponseFailed);
        }

        Ok(())
    }

    pub fn update_block_height(&mut self, block_height: BlockHeight) {
        self.swarm.behaviour_mut().update_block_height(block_height)
    }

    /// The report is forwarded to gossipsub behaviour
    /// If acceptance is "Rejected" the gossipsub peer score is calculated
    /// And if it's below allowed threshold the peer is banned
    pub fn report_message_validation_result(
        &mut self,
        msg_id: &MessageId,
        propagation_source: PeerId,
        mut acceptance: MessageAcceptance,
    ) {
        // Even invalid transactions shouldn't affect reserved peer reputation.
        if let MessageAcceptance::Reject = acceptance {
            if self.peer_manager.is_reserved(&propagation_source) {
                acceptance = MessageAcceptance::Ignore;
            }
        }

        if let Some(gossip_score) = self
            .swarm
            .behaviour_mut()
            .report_message_validation_result(msg_id, &propagation_source, acceptance)
        {
            self.peer_manager.handle_gossip_score_update(
                propagation_source,
                gossip_score,
                &mut self.swarm,
            );
        }
    }

    #[cfg(test)]
    pub fn get_peer_score(&self, peer_id: &PeerId) -> Option<f64> {
        self.swarm.behaviour().get_peer_score(peer_id)
    }

    /// Report application score
    /// If application peer score is below allowed threshold
    /// the peer is banned
    pub fn report_peer(
        &mut self,
        peer_id: PeerId,
        app_score: AppScore,
        reporting_service: &str,
    ) {
        self.peer_manager.update_app_score(
            peer_id,
            app_score,
            reporting_service,
            &mut self.swarm,
        );
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
            _ => {
                self.update_libp2p_metrics(&event);
                None
            }
        }
    }

    pub fn peer_manager(&self) -> &PeerManager {
        &self.peer_manager
    }

    fn get_topic_tag(&self, topic_hash: &TopicHash) -> Option<GossipTopicTag> {
        let topic = self
            .network_metadata
            .gossipsub_data
            .topics
            .get_gossipsub_tag(topic_hash);
        if topic.is_none() {
            warn!(target: "fuel-p2p", "GossipTopicTag does not exist for {:?}", &topic_hash);
        }
        topic
    }

    fn handle_behaviour_event(
        &mut self,
        event: FuelBehaviourEvent,
    ) -> Option<FuelP2PEvent> {
        match event {
            FuelBehaviourEvent::Gossipsub(event) => {
                self.update_libp2p_metrics(&event);
                self.handle_gossipsub_event(event)
            }
            FuelBehaviourEvent::PeerReport(event) => self.handle_peer_report_event(event),
            FuelBehaviourEvent::RequestResponse(event) => {
                self.handle_request_response_event(event)
            }
            FuelBehaviourEvent::Identify(event) => {
                self.update_libp2p_metrics(&event);
                self.handle_identify_event(event)
            }
            FuelBehaviourEvent::Heartbeat(event) => self.handle_heartbeat_event(event),
            FuelBehaviourEvent::Discovery(event) => {
                self.update_libp2p_metrics(&event);
                None
            }
            _ => None,
        }
    }

    fn handle_gossipsub_event(
        &mut self,
        event: gossipsub::Event,
    ) -> Option<FuelP2PEvent> {
        match event {
            gossipsub::Event::Message {
                propagation_source,
                message,
                message_id,
            } => {
                let correct_topic = self.get_topic_tag(&message.topic)?;
                match self.gossipsub_codec.decode(&message.data, correct_topic) {
                    Ok(decoded_message) => Some(FuelP2PEvent::GossipsubMessage {
                        peer_id: propagation_source,
                        message_id,
                        topic_hash: message.topic,
                        message: decoded_message,
                    }),
                    Err(err) => {
                        warn!(target: "fuel-p2p", "Failed to decode a message. ID: {}, Message: {:?} with error: {:?}", message_id, &message.data, err);

                        self.report_message_validation_result(
                            &message_id,
                            propagation_source,
                            MessageAcceptance::Reject,
                        );
                        None
                    }
                }
            }
            gossipsub::Event::Subscribed { peer_id, topic } => {
                let tag = self.get_topic_tag(&topic)?;
                Some(FuelP2PEvent::NewSubscription { peer_id, tag })
            }
            _ => None,
        }
    }

    fn handle_peer_report_event(
        &mut self,
        event: PeerReportEvent,
    ) -> Option<FuelP2PEvent> {
        match event {
            PeerReportEvent::PerformDecay => {
                self.peer_manager.batch_update_score_with_decay()
            }
            PeerReportEvent::PeerConnected { peer_id } => {
                if self.peer_manager.handle_peer_connected(&peer_id) {
                    let _ = self.swarm.disconnect_peer_id(peer_id);
                } else {
                    return Some(FuelP2PEvent::PeerConnected(peer_id));
                }
            }
            PeerReportEvent::PeerDisconnected { peer_id } => {
                self.peer_manager.handle_peer_disconnect(peer_id);
                return Some(FuelP2PEvent::PeerDisconnected(peer_id));
            }
        }
        None
    }

    fn handle_request_response_event(
        &mut self,
        event: request_response::Event<RequestMessage, V2ResponseMessage>,
    ) -> Option<FuelP2PEvent> {
        match event {
            request_response::Event::Message { peer, message } => match message {
                request_response::Message::Request {
                    request,
                    channel,
                    request_id,
                } => {
                    self.inbound_requests_table.insert(request_id, channel);

                    return Some(FuelP2PEvent::InboundRequestMessage {
                        request_id,
                        request_message: request,
                    });
                }
                request_response::Message::Response {
                    request_id,
                    response,
                } => {
                    let Some(channel) = self.outbound_requests_table.remove(&request_id)
                    else {
                        debug!("Send channel not found for {:?}", request_id);
                        return None;
                    };

                    let send_ok = match channel {
                        ResponseSender::SealedHeaders(c) => match response {
                            V2ResponseMessage::SealedHeaders(v) => {
                                c.send(Ok((peer, Ok(v)))).is_ok()
                            }
                            _ => {
                                warn!(
                                    "Invalid response type received for request {:?}",
                                    request_id
                                );
                                c.send(Ok((peer, Err(ResponseError::TypeMismatch))))
                                    .is_ok()
                            }
                        },
                        ResponseSender::Transactions(c) => match response {
                            V2ResponseMessage::Transactions(v) => {
                                c.send(Ok((peer, Ok(v)))).is_ok()
                            }
                            _ => {
                                warn!(
                                    "Invalid response type received for request {:?}",
                                    request_id
                                );
                                c.send(Ok((peer, Err(ResponseError::TypeMismatch))))
                                    .is_ok()
                            }
                        },
                        ResponseSender::TransactionsFromPeer(c) => match response {
                            V2ResponseMessage::Transactions(v) => {
                                c.send((peer, Ok(v))).is_ok()
                            }
                            _ => {
                                warn!(
                                    "Invalid response type received for request {:?}",
                                    request_id
                                );
                                c.send((peer, Err(ResponseError::TypeMismatch))).is_ok()
                            }
                        },
                        ResponseSender::TxPoolAllTransactionsIds(c) => match response {
                            V2ResponseMessage::TxPoolAllTransactionsIds(v) => {
                                c.send((peer, Ok(v))).is_ok()
                            }
                            _ => {
                                warn!(
                                    "Invalid response type received for request {:?}",
                                    request_id
                                );
                                c.send((peer, Err(ResponseError::TypeMismatch))).is_ok()
                            }
                        },
                        ResponseSender::TxPoolFullTransactions(c) => match response {
                            V2ResponseMessage::TxPoolFullTransactions(v) => {
                                c.send((peer, Ok(v))).is_ok()
                            }
                            _ => {
                                warn!(
                                    "Invalid response type received for request {:?}",
                                    request_id
                                );
                                c.send((peer, Err(ResponseError::TypeMismatch))).is_ok()
                            }
                        },
                    };

                    if !send_ok {
                        warn!("Failed to send through the channel for {:?}", request_id);
                    }
                }
            },
            request_response::Event::InboundFailure {
                peer,
                error,
                request_id,
            } => {
                tracing::error!("RequestResponse inbound error for peer: {:?} with id: {:?} and error: {:?}", peer, request_id, error);

                // Drop the channel, as we can't send a response
                let _ = self.inbound_requests_table.remove(&request_id);
            }
            request_response::Event::OutboundFailure {
                peer,
                error,
                request_id,
            } => {
                // If the remote peer doesn't support the protocol, it is better to disconnect
                // to find another peer that supports it.
                if let OutboundFailure::UnsupportedProtocols = error {
                    let _ = self.swarm.disconnect_peer_id(peer);
                }

                tracing::error!("RequestResponse outbound error for peer: {:?} with id: {:?} and error: {:?}", peer, request_id, error);

                if let Some(channel) = self.outbound_requests_table.remove(&request_id) {
                    match channel {
                        ResponseSender::SealedHeaders(c) => {
                            let _ = c.send(Ok((peer, Err(ResponseError::P2P(error)))));
                        }
                        ResponseSender::Transactions(c) => {
                            let _ = c.send(Ok((peer, Err(ResponseError::P2P(error)))));
                        }
                        ResponseSender::TransactionsFromPeer(c) => {
                            let _ = c.send((peer, Err(ResponseError::P2P(error))));
                        }
                        ResponseSender::TxPoolAllTransactionsIds(c) => {
                            let _ = c.send((peer, Err(ResponseError::P2P(error))));
                        }
                        ResponseSender::TxPoolFullTransactions(c) => {
                            let _ = c.send((peer, Err(ResponseError::P2P(error))));
                        }
                    };
                }
            }
            _ => {}
        }
        None
    }

    fn handle_identify_event(&mut self, event: identify::Event) -> Option<FuelP2PEvent> {
        match event {
            identify::Event::Received { peer_id, info, .. } => {
                self.update_metrics(increment_unique_peers);

                let mut addresses = info.listen_addrs;
                let agent_version = info.agent_version;

                if addresses.len() > MAX_IDENTIFY_ADDRESSES {
                    let protocol_version = info.protocol_version;
                    debug!(
                        target: "fuel-p2p",
                        "Node {:?} has reported more than {} addresses; it is identified by {:?} and {:?}",
                        peer_id, MAX_IDENTIFY_ADDRESSES, protocol_version, agent_version
                    );
                    addresses.truncate(MAX_IDENTIFY_ADDRESSES);
                }

                self.peer_manager.handle_peer_identified(
                    &peer_id,
                    addresses.clone(),
                    agent_version,
                );

                self.swarm
                    .behaviour_mut()
                    .add_addresses_to_discovery(&peer_id, addresses);
            }
            identify::Event::Sent { .. } => {}
            identify::Event::Pushed { .. } => {}
            identify::Event::Error {
                connection_id,
                peer_id,
                error,
            } => {
                debug!(target: "fuel-p2p", "Identification with peer {:?} with connection id {:?} failed => {}", peer_id, connection_id, error);
            }
        }
        None
    }

    fn handle_heartbeat_event(
        &mut self,
        event: heartbeat::Event,
    ) -> Option<FuelP2PEvent> {
        let heartbeat::Event {
            peer_id,
            latest_block_height,
        } = event;
        self.peer_manager
            .handle_peer_info_updated(&peer_id, latest_block_height);

        Some(FuelP2PEvent::PeerInfoUpdated {
            peer_id,
            block_height: latest_block_height,
        })
    }
}
