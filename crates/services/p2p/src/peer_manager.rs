use crate::{
    gossipsub_config::GRAYLIST_THRESHOLD,
    peer_manager::heartbeat_data::HeartbeatData,
};
use fuel_core_services::seqlock::{
    SeqLock,
    SeqLockReader,
    SeqLockWriter,
};
use fuel_core_types::{
    fuel_types::BlockHeight,
    services::p2p::peer_reputation::{
        AppScore,
        DECAY_APP_SCORE,
        DEFAULT_APP_SCORE,
        MAX_APP_SCORE,
        MIN_APP_SCORE,
    },
};
use libp2p::{
    Multiaddr,
    PeerId,
};
use rand::seq::IteratorRandom;
use std::collections::{
    HashMap,
    HashSet,
};
use tracing::{
    debug,
    info,
};

pub mod heartbeat_data;

/// At this point we better just ban the peer
const MIN_GOSSIPSUB_SCORE_BEFORE_BAN: AppScore = GRAYLIST_THRESHOLD;

// Info about a single Peer that we're connected to
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub peer_addresses: HashSet<Multiaddr>,
    pub client_version: Option<String>,
    pub heartbeat_data: HeartbeatData,
    pub score: AppScore,
}

impl PeerInfo {
    pub fn new(heartbeat_avg_window: u32) -> Self {
        Self {
            peer_addresses: HashSet::new(),
            client_version: None,
            heartbeat_data: HeartbeatData::new(heartbeat_avg_window),
            score: DEFAULT_APP_SCORE,
        }
    }
}

/// Manages Peers and their events
#[derive(Debug)]
pub struct PeerManager {
    score_config: ScoreConfig,
    non_reserved_connected_peers: HashMap<PeerId, PeerInfo>,
    reserved_connected_peers: HashMap<PeerId, PeerInfo>,
    reserved_peers: HashSet<PeerId>,
    connection_state_writer: SeqLockWriter<ConnectionState>,
    max_non_reserved_peers: usize,
    reserved_peers_updates: tokio::sync::broadcast::Sender<usize>,
}

impl PeerManager {
    pub fn new(
        reserved_peers_updates: tokio::sync::broadcast::Sender<usize>,
        reserved_peers: HashSet<PeerId>,
        connection_state_writer: SeqLockWriter<ConnectionState>,
        max_non_reserved_peers: usize,
    ) -> Self {
        Self {
            score_config: ScoreConfig::default(),
            non_reserved_connected_peers: HashMap::with_capacity(max_non_reserved_peers),
            reserved_connected_peers: HashMap::with_capacity(reserved_peers.len()),
            reserved_peers,
            connection_state_writer,
            max_non_reserved_peers,
            reserved_peers_updates,
        }
    }

    pub fn reserved_peers_updates(&self) -> tokio::sync::broadcast::Sender<usize> {
        self.reserved_peers_updates.clone()
    }

    pub fn is_reserved(&self, peer_id: &PeerId) -> bool {
        self.reserved_peers.contains(peer_id)
    }

    pub fn handle_gossip_score_update<T: Punisher>(
        &self,
        peer_id: PeerId,
        gossip_score: f64,
        punisher: &mut T,
    ) {
        if gossip_score < self.score_config.min_gossip_score_allowed
            && !self.reserved_peers.contains(&peer_id)
        {
            punisher.ban_peer(peer_id);
        }
    }

    pub fn handle_peer_info_updated(
        &mut self,
        peer_id: &PeerId,
        block_height: BlockHeight,
    ) {
        if let Some(time_elapsed) = self
            .get_peer_info(peer_id)
            .map(|info| info.heartbeat_data.duration_since_last_heartbeat())
        {
            debug!(target: "fuel-p2p", "Previous heartbeat happened {:?} milliseconds ago", time_elapsed.as_millis());
        }

        let peers = self.get_assigned_peer_table_mut(peer_id);
        update_heartbeat(peers, peer_id, block_height);
    }

    /// Returns `true` signaling that the peer should be disconnected
    pub fn handle_peer_connected(&mut self, peer_id: &PeerId) -> bool {
        self.handle_initial_connection(peer_id)
    }

    pub fn handle_peer_identified(
        &mut self,
        peer_id: &PeerId,
        addresses: Vec<Multiaddr>,
        agent_version: String,
    ) {
        let peers = self.get_assigned_peer_table_mut(peer_id);
        insert_client_version(peers, peer_id, agent_version);
        insert_peer_addresses(peers, peer_id, addresses);
    }

    pub fn batch_update_score_with_decay(&mut self) {
        for peer_info in self.non_reserved_connected_peers.values_mut() {
            peer_info.score *= DECAY_APP_SCORE;
        }
    }

    /// Applies the reported `score` to the peer's reputation, banning the peer
    /// when its reputation falls below the minimum allowed score.
    ///
    /// Reserved peers are intentionally out of reach of this function: only
    /// `non_reserved_connected_peers` is looked up, so a report against a
    /// reserved peer is a no-op. Reserved peers are picked by the operator, and
    /// must never be banned because of their reputation.
    pub fn update_app_score<T: Punisher>(
        &mut self,
        peer_id: PeerId,
        score: AppScore,
        reporting_service: &str,
        punisher: &mut T,
    ) {
        match self.non_reserved_connected_peers.get_mut(&peer_id) {
            Some(peer) => {
                // score should not go over `max_score`
                let new_score = self.score_config.max_app_score.min(peer.score + score);
                peer.score = new_score;

                info!(target: "fuel-p2p", "{reporting_service} updated {peer_id} with new score {score}");

                if new_score < self.score_config.min_app_score_allowed {
                    punisher.ban_peer(peer_id);
                }
            }
            _ => {
                log_missing_peer(&peer_id);
            }
        }
    }

    pub fn total_peers_connected(&self) -> usize {
        self.reserved_connected_peers
            .len()
            .saturating_add(self.non_reserved_connected_peers.len())
    }

    pub fn get_peers_ids(&self) -> impl Iterator<Item = &PeerId> {
        self.non_reserved_connected_peers
            .keys()
            .chain(self.reserved_connected_peers.keys())
    }

    pub fn get_peer_info(&self, peer_id: &PeerId) -> Option<&PeerInfo> {
        if self.reserved_peers.contains(peer_id) {
            return self.reserved_connected_peers.get(peer_id);
        }
        self.non_reserved_connected_peers.get(peer_id)
    }

    pub fn get_all_peers(&self) -> impl Iterator<Item = (&PeerId, &PeerInfo)> {
        self.non_reserved_connected_peers
            .iter()
            .chain(self.reserved_connected_peers.iter())
    }

    /// Max heartbeat height among currently connected reserved peers.
    pub fn reserved_peer_network_height(&self) -> Option<BlockHeight> {
        self.reserved_connected_peers
            .values()
            .filter_map(|info| info.heartbeat_data.block_height)
            .max()
    }

    /// Handles on peer's last connection getting disconnected
    /// Returns 'true' signaling we should try reconnecting
    pub fn handle_peer_disconnect(&mut self, peer_id: PeerId) -> bool {
        // try immediate reconnect if it's a reserved peer
        let is_reserved = self.reserved_peers.contains(&peer_id);

        if !is_reserved {
            // check were all the slots taken prior to this disconnect
            let all_slots_taken = self.max_non_reserved_peers
                == self.non_reserved_connected_peers.len().saturating_add(1);

            if self.non_reserved_connected_peers.remove(&peer_id).is_some()
                && all_slots_taken
            {
                // since all the slots were full prior to this disconnect
                // let's allow new peer non-reserved peers connections
                self.connection_state_writer.write(|data| {
                    data.allow_new_peers();
                });
            }

            false
        } else if self.reserved_connected_peers.remove(&peer_id).is_some() {
            self.send_reserved_peers_update();
            true
        } else {
            false
        }
    }

    /// Find a peer that is holding the given block height.
    ///
    /// Reserved peers are preferred over the non-reserved ones: they are picked
    /// by the operator and are the only peers we can rely on, while a failed
    /// request to a broken non-reserved peer stalls the block import for a full
    /// request timeout. A non-reserved peer is only used when no reserved peer
    /// reports holding the requested height.
    pub fn get_peer_id_with_height(&self, height: &BlockHeight) -> Option<PeerId> {
        let mut range = rand::thread_rng();
        // TODO: Optimize the selection of the peer.
        //  We can store pair `(peer id, height)` for all nodes(reserved and not) in the
        //  https://docs.rs/sorted-vec/latest/sorted_vec/struct.SortedVec.html
        peers_with_height(&self.reserved_connected_peers, height)
            .choose(&mut range)
            .or_else(|| {
                peers_with_height(&self.non_reserved_connected_peers, height)
                    .choose(&mut range)
            })
    }

    /// Handles the first connection established with a Peer
    fn handle_initial_connection(&mut self, peer_id: &PeerId) -> bool {
        const HEARTBEAT_AVG_WINDOW: u32 = 10;
        let is_reserved = self.reserved_peers.contains(peer_id);

        // if the connected Peer is not from the reserved peers
        if !is_reserved && !self.non_reserved_connected_peers.contains_key(peer_id) {
            let non_reserved_peers_connected = self.non_reserved_connected_peers.len();
            // check if all the slots are already taken
            if non_reserved_peers_connected >= self.max_non_reserved_peers {
                // Too many peers already connected, disconnect the Peer
                return true;
            }

            if non_reserved_peers_connected.saturating_add(1)
                == self.max_non_reserved_peers
            {
                // this is the last non-reserved peer allowed
                self.connection_state_writer.write(|data| {
                    data.deny_new_peers();
                });
            }

            self.non_reserved_connected_peers
                .insert(*peer_id, PeerInfo::new(HEARTBEAT_AVG_WINDOW));
        } else if is_reserved && !self.reserved_connected_peers.contains_key(peer_id) {
            self.reserved_connected_peers
                .insert(*peer_id, PeerInfo::new(HEARTBEAT_AVG_WINDOW));

            self.send_reserved_peers_update();
        }

        false
    }

    fn send_reserved_peers_update(&self) {
        let _ = self
            .reserved_peers_updates
            .send(self.reserved_connected_peers.len());
    }

    fn get_assigned_peer_table_mut(
        &mut self,
        peer_id: &PeerId,
    ) -> &mut HashMap<PeerId, PeerInfo> {
        if self.reserved_peers.contains(peer_id) {
            &mut self.reserved_connected_peers
        } else {
            &mut self.non_reserved_connected_peers
        }
    }
}

fn insert_peer_addresses(
    peers: &mut HashMap<PeerId, PeerInfo>,
    peer_id: &PeerId,
    addresses: Vec<Multiaddr>,
) {
    if let Some(peer) = peers.get_mut(peer_id) {
        for address in addresses {
            peer.peer_addresses.insert(address);
        }
    } else {
        log_missing_peer(peer_id);
    }
}

/// Returns the peers that reported holding at least the given block height.
fn peers_with_height<'a>(
    peers: &'a HashMap<PeerId, PeerInfo>,
    height: &'a BlockHeight,
) -> impl Iterator<Item = PeerId> + 'a {
    peers
        .iter()
        .filter(move |(_, peer_info)| {
            peer_info.heartbeat_data.block_height >= Some(*height)
        })
        .map(|(peer_id, _)| *peer_id)
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ConnectionState {
    peers_allowed: bool,
}

impl ConnectionState {
    pub fn new() -> (
        SeqLockWriter<ConnectionState>,
        SeqLockReader<ConnectionState>,
    ) {
        // ConnectionState < 64 bytes, so it's safe to use SeqLock
        unsafe {
            SeqLock::new(Self {
                peers_allowed: true,
            })
        }
    }

    pub fn available_slot(&self) -> bool {
        self.peers_allowed
    }

    fn allow_new_peers(&mut self) {
        self.peers_allowed = true;
    }

    fn deny_new_peers(&mut self) {
        self.peers_allowed = false;
    }
}

fn update_heartbeat(
    peers: &mut HashMap<PeerId, PeerInfo>,
    peer_id: &PeerId,
    block_height: BlockHeight,
) {
    if let Some(peer) = peers.get_mut(peer_id) {
        peer.heartbeat_data.update(block_height);
    } else {
        log_missing_peer(peer_id);
    }
}

fn insert_client_version(
    peers: &mut HashMap<PeerId, PeerInfo>,
    peer_id: &PeerId,
    client_version: String,
) {
    if let Some(peer) = peers.get_mut(peer_id) {
        peer.client_version = Some(client_version);
    } else {
        log_missing_peer(peer_id);
    }
}

fn log_missing_peer(peer_id: &PeerId) {
    debug!(target: "fuel-p2p", "Peer with PeerId: {:?} is not among the connected peers", peer_id)
}

#[derive(Clone, Debug, Copy)]
struct ScoreConfig {
    max_app_score: AppScore,
    min_app_score_allowed: AppScore,
    min_gossip_score_allowed: f64,
}

impl Default for ScoreConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl ScoreConfig {
    pub fn new() -> Self {
        Self {
            max_app_score: MAX_APP_SCORE,
            min_app_score_allowed: MIN_APP_SCORE,
            min_gossip_score_allowed: MIN_GOSSIPSUB_SCORE_BEFORE_BAN,
        }
    }
}

pub trait Punisher {
    fn ban_peer(&mut self, peer_id: PeerId);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::p2p_service::REQUEST_FAILURE_PENALTY;

    /// The default `--request-timeout`, and so the shortest interval at which a
    /// peer that keeps timing out can be reported.
    const REQUEST_TIMEOUT_IN_SECONDS: usize = 20;

    /// A `Punisher` that only records the peers it was asked to ban.
    #[derive(Default)]
    struct BanTracker {
        banned_peers: Vec<PeerId>,
    }

    impl Punisher for BanTracker {
        fn ban_peer(&mut self, peer_id: PeerId) {
            self.banned_peers.push(peer_id);
        }
    }

    fn get_random_peers(size: usize) -> Vec<PeerId> {
        (0..size).map(|_| PeerId::random()).collect()
    }

    fn initialize_peer_manager(
        reserved_peers: Vec<PeerId>,
        max_non_reserved_peers: usize,
    ) -> PeerManager {
        let (connection_state_writer, _) = ConnectionState::new();
        let (sender, _) =
            tokio::sync::broadcast::channel(reserved_peers.len().saturating_add(1));

        PeerManager::new(
            sender,
            reserved_peers.into_iter().collect(),
            connection_state_writer,
            max_non_reserved_peers,
        )
    }

    #[test]
    fn only_allowed_number_of_non_reserved_peers_is_connected() {
        let max_non_reserved_peers = 5;
        let mut peer_manager = initialize_peer_manager(vec![], max_non_reserved_peers);

        let random_peers = get_random_peers(max_non_reserved_peers * 2);

        // try connecting all the random peers
        for peer_id in &random_peers {
            peer_manager.handle_initial_connection(peer_id);
        }

        assert_eq!(peer_manager.total_peers_connected(), max_non_reserved_peers);
    }

    #[test]
    fn only_reserved_peers_are_connected() {
        let max_non_reserved_peers = 0;
        let reserved_peers = get_random_peers(5);
        let mut peer_manager =
            initialize_peer_manager(reserved_peers.clone(), max_non_reserved_peers);

        // try connecting all the reserved peers
        for peer_id in &reserved_peers {
            peer_manager.handle_initial_connection(peer_id);
        }

        assert_eq!(peer_manager.total_peers_connected(), reserved_peers.len());

        // try connecting random peers
        let random_peers = get_random_peers(10);
        for peer_id in &random_peers {
            peer_manager.handle_initial_connection(peer_id);
        }

        // the number should stay the same
        assert_eq!(peer_manager.total_peers_connected(), reserved_peers.len());
    }

    #[test]
    fn non_reserved_peer_does_not_take_reserved_slot() {
        let max_non_reserved_peers = 5;
        let reserved_peers = get_random_peers(5);
        let mut peer_manager =
            initialize_peer_manager(reserved_peers.clone(), max_non_reserved_peers);

        // try connecting all the reserved peers
        for peer_id in &reserved_peers {
            peer_manager.handle_initial_connection(peer_id);
        }

        // disconnect a single reserved peer
        peer_manager.handle_peer_disconnect(*reserved_peers.first().unwrap());

        // try connecting random peers
        let random_peers = get_random_peers(max_non_reserved_peers * 2);
        for peer_id in &random_peers {
            peer_manager.handle_initial_connection(peer_id);
        }

        // there should be an available slot for a reserved peer
        assert_eq!(
            peer_manager.total_peers_connected(),
            reserved_peers.len() - 1 + max_non_reserved_peers
        );

        // reconnect the disconnected reserved peer
        peer_manager.handle_initial_connection(reserved_peers.first().unwrap());

        // all the slots should be taken now
        assert_eq!(
            peer_manager.total_peers_connected(),
            reserved_peers.len() + max_non_reserved_peers
        );
    }

    #[test]
    fn reserved_peer_is_preferred_when_it_holds_the_requested_height() {
        let reserved_peers = get_random_peers(2);
        let mut peer_manager = initialize_peer_manager(reserved_peers.clone(), 5);

        // all the peers, reserved and not, can serve the requested height
        let non_reserved_peers = get_random_peers(5);
        for peer_id in reserved_peers.iter().chain(non_reserved_peers.iter()) {
            peer_manager.handle_initial_connection(peer_id);
            peer_manager.handle_peer_info_updated(peer_id, 10u32.into());
        }

        // only the reserved ones are ever picked
        for _ in 0..100 {
            let peer_id = peer_manager
                .get_peer_id_with_height(&5u32.into())
                .expect("A peer with the requested height is connected");

            assert!(reserved_peers.contains(&peer_id));
        }
    }

    #[test]
    fn non_reserved_peer_is_used_when_no_reserved_peer_holds_the_requested_height() {
        let reserved_peers = get_random_peers(2);
        let mut peer_manager = initialize_peer_manager(reserved_peers.clone(), 5);

        // the reserved peers lag behind the requested height
        for peer_id in &reserved_peers {
            peer_manager.handle_initial_connection(peer_id);
            peer_manager.handle_peer_info_updated(peer_id, 4u32.into());
        }

        let non_reserved_peers = get_random_peers(3);
        for peer_id in &non_reserved_peers {
            peer_manager.handle_initial_connection(peer_id);
            peer_manager.handle_peer_info_updated(peer_id, 10u32.into());
        }

        for _ in 0..100 {
            let peer_id = peer_manager
                .get_peer_id_with_height(&5u32.into())
                .expect("A non-reserved peer with the requested height is connected");

            assert!(non_reserved_peers.contains(&peer_id));
        }

        // and nobody is picked for a height that no peer holds
        assert_eq!(peer_manager.get_peer_id_with_height(&11u32.into()), None);
    }

    #[test]
    fn reputation_reports_never_affect_reserved_peers() {
        let reserved_peer = PeerId::random();
        let non_reserved_peer = PeerId::random();
        let mut peer_manager = initialize_peer_manager(vec![reserved_peer], 5);

        peer_manager.handle_initial_connection(&reserved_peer);
        peer_manager.handle_initial_connection(&non_reserved_peer);

        // a penalty large enough to ban a peer on its own
        let penalty = MIN_APP_SCORE - 1.;
        let mut punisher = BanTracker::default();
        peer_manager.update_app_score(reserved_peer, penalty, "test", &mut punisher);
        peer_manager.update_app_score(non_reserved_peer, penalty, "test", &mut punisher);

        // the reserved peer is untouched, while the non-reserved one is banned
        assert_eq!(
            peer_manager.get_peer_info(&reserved_peer).unwrap().score,
            DEFAULT_APP_SCORE
        );
        assert_eq!(
            peer_manager
                .get_peer_info(&non_reserved_peer)
                .unwrap()
                .score,
            penalty
        );
        assert_eq!(punisher.banned_peers, vec![non_reserved_peer]);
    }

    #[test]
    fn single_request_failure_does_not_ban_the_peer() {
        let peer_id = PeerId::random();
        let mut peer_manager = initialize_peer_manager(vec![], 5);
        peer_manager.handle_initial_connection(&peer_id);

        let mut punisher = BanTracker::default();
        peer_manager.update_app_score(
            peer_id,
            REQUEST_FAILURE_PENALTY,
            "test",
            &mut punisher,
        );

        assert!(punisher.banned_peers.is_empty());

        // and the failure is almost forgotten after 44 seconds of decay
        for _ in 0..44 {
            peer_manager.batch_update_score_with_decay();
        }

        assert!(peer_manager.get_peer_info(&peer_id).unwrap().score > -0.5);
    }

    #[test]
    fn repeated_request_failures_ban_the_peer_despite_the_decay() {
        let peer_id = PeerId::random();
        let mut peer_manager = initialize_peer_manager(vec![], 5);
        peer_manager.handle_initial_connection(&peer_id);

        // the peer fails a request every time the request times out
        let mut punisher = BanTracker::default();
        for _ in 0..2 {
            peer_manager.update_app_score(
                peer_id,
                REQUEST_FAILURE_PENALTY,
                "test",
                &mut punisher,
            );

            for _ in 0..REQUEST_TIMEOUT_IN_SECONDS {
                peer_manager.batch_update_score_with_decay();
            }
        }

        assert_eq!(punisher.banned_peers, vec![peer_id]);
    }
}
