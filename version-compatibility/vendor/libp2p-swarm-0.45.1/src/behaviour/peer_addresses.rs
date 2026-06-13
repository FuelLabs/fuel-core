use crate::behaviour::FromSwarm;
use crate::{DialError, DialFailure, NewExternalAddrOfPeer};

use libp2p_core::Multiaddr;
use libp2p_identity::PeerId;

use lru::LruCache;

use std::num::NonZeroUsize;

/// Struct for tracking peers' external addresses of the [`Swarm`](crate::Swarm).
#[derive(Debug)]
pub struct PeerAddresses(LruCache<PeerId, LruCache<Multiaddr, ()>>);

impl PeerAddresses {
    /// Creates a [`PeerAddresses`] cache with capacity for the given number of peers.
    ///
    /// For each peer, we will at most store 10 addresses.
    pub fn new(number_of_peers: NonZeroUsize) -> Self {
        Self(LruCache::new(number_of_peers))
    }

    /// Feed a [`FromSwarm`] event to this struct.
    ///
    /// Returns whether the event changed peer's known external addresses.
    pub fn on_swarm_event(&mut self, event: &FromSwarm) -> bool {
        match event {
            FromSwarm::NewExternalAddrOfPeer(NewExternalAddrOfPeer { peer_id, addr }) => {
                self.add(*peer_id, (*addr).clone())
            }
            FromSwarm::DialFailure(DialFailure {
                peer_id: Some(peer_id),
                error: DialError::Transport(errors),
                ..
            }) => {
                for (addr, _error) in errors {
                    self.remove(peer_id, addr);
                }
                true
            }
            _ => false,
        }
    }

    /// Adds address to cache.
    /// Appends address to the existing set if peer addresses already exist.
    /// Creates a new cache entry for peer_id if no addresses are present.
    /// Returns true if the newly added address was not previously in the cache.
    ///
    pub fn add(&mut self, peer: PeerId, address: Multiaddr) -> bool {
        match prepare_addr(&peer, &address) {
            Ok(address) => {
                if let Some(cached) = self.0.get_mut(&peer) {
                    cached.put(address, ()).is_none()
                } else {
                    let mut set = LruCache::new(NonZeroUsize::new(10).expect("10 > 0"));
                    set.put(address, ());
                    self.0.put(peer, set);

                    true
                }
            }
            Err(_) => false,
        }
    }

    /// Returns peer's external addresses.
    pub fn get(&mut self, peer: &PeerId) -> impl Iterator<Item = Multiaddr> + '_ {
        self.0
            .get(peer)
            .into_iter()
            .flat_map(|c| c.iter().map(|(m, ())| m))
            .cloned()
    }

    /// Removes address from peer addresses cache.
    /// Returns true if the address was removed.
    pub fn remove(&mut self, peer: &PeerId, address: &Multiaddr) -> bool {
        match self.0.get_mut(peer) {
            Some(addrs) => match prepare_addr(peer, address) {
                Ok(address) => addrs.pop(&address).is_some(),
                Err(_) => false,
            },
            None => false,
        }
    }
}

fn prepare_addr(peer: &PeerId, addr: &Multiaddr) -> Result<Multiaddr, Multiaddr> {
    addr.clone().with_p2p(*peer)
}

impl Default for PeerAddresses {
    fn default() -> Self {
        Self(LruCache::new(NonZeroUsize::new(100).unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    use crate::ConnectionId;
    use libp2p_core::{
        multiaddr::Protocol,
        transport::{memory::MemoryTransportError, TransportError},
    };

    use once_cell::sync::Lazy;

    #[test]
    fn new_peer_addr_returns_correct_changed_value() {
        let mut cache = PeerAddresses::default();
        let peer_id = PeerId::random();

        let event = new_external_addr_of_peer1(peer_id);

        let changed = cache.on_swarm_event(&event);
        assert!(changed);

        let changed = cache.on_swarm_event(&event);
        assert!(!changed);
    }

    #[test]
    fn new_peer_addr_saves_peer_addrs() {
        let mut cache = PeerAddresses::default();
        let peer_id = PeerId::random();
        let event = new_external_addr_of_peer1(peer_id);

        let changed = cache.on_swarm_event(&event);
        assert!(changed);

        let addr1 = MEMORY_ADDR_1000.clone().with_p2p(peer_id).unwrap();
        let expected = cache.get(&peer_id).collect::<Vec<Multiaddr>>();
        assert_eq!(expected, vec![addr1]);

        let event = new_external_addr_of_peer2(peer_id);
        let changed = cache.on_swarm_event(&event);

        let addr1 = MEMORY_ADDR_1000.clone().with_p2p(peer_id).unwrap();
        let addr2 = MEMORY_ADDR_2000.clone().with_p2p(peer_id).unwrap();

        let expected_addrs = cache.get(&peer_id).collect::<Vec<Multiaddr>>();
        assert!(expected_addrs.contains(&addr1));
        assert!(expected_addrs.contains(&addr2));

        let expected = cache.get(&peer_id).collect::<Vec<Multiaddr>>().len();
        assert_eq!(expected, 2);

        assert!(changed);
    }

    #[test]
    fn existing_addr_is_not_added_to_cache() {
        let mut cache = PeerAddresses::default();
        let peer_id = PeerId::random();

        let event = new_external_addr_of_peer1(peer_id);

        let addr1 = MEMORY_ADDR_1000.clone().with_p2p(peer_id).unwrap();
        let changed = cache.on_swarm_event(&event);
        let expected = cache.get(&peer_id).collect::<Vec<Multiaddr>>();
        assert!(changed);
        assert_eq!(expected, vec![addr1]);

        let addr1 = MEMORY_ADDR_1000.clone().with_p2p(peer_id).unwrap();
        let changed = cache.on_swarm_event(&event);
        let expected = cache.get(&peer_id).collect::<Vec<Multiaddr>>();
        assert!(!changed);
        assert_eq!(expected, [addr1]);
    }

    #[test]
    fn addresses_of_peer_are_removed_when_received_dial_failure() {
        let mut cache = PeerAddresses::default();
        let peer_id = PeerId::random();

        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/8080".parse().unwrap();
        let addr2: Multiaddr = "/ip4/127.0.0.1/tcp/8081".parse().unwrap();
        let addr3: Multiaddr = "/ip4/127.0.0.1/tcp/8082".parse().unwrap();

        cache.add(peer_id, addr.clone());
        cache.add(peer_id, addr2.clone());
        cache.add(peer_id, addr3.clone());

        let error = DialError::Transport(prepare_errors(vec![addr, addr3]));

        let event = FromSwarm::DialFailure(DialFailure {
            peer_id: Some(peer_id),
            error: &error,
            connection_id: ConnectionId::new_unchecked(8),
        });

        let changed = cache.on_swarm_event(&event);

        assert!(changed);

        let cached = cache.get(&peer_id).collect::<Vec<Multiaddr>>();
        let expected = prepare_expected_addrs(peer_id, [addr2].into_iter());

        assert_eq!(cached, expected);
    }

    #[test]
    fn remove_removes_address_if_present() {
        let mut cache = PeerAddresses::default();
        let peer_id = PeerId::random();
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/8080".parse().unwrap();

        cache.add(peer_id, addr.clone());

        assert!(cache.remove(&peer_id, &addr));
    }

    #[test]
    fn remove_returns_false_if_address_not_present() {
        let mut cache = PeerAddresses::default();
        let peer_id = PeerId::random();
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/8080".parse().unwrap();

        assert!(!cache.remove(&peer_id, &addr));
    }

    #[test]
    fn remove_returns_false_if_peer_not_present() {
        let mut cache = PeerAddresses::default();
        let peer_id = PeerId::random();
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/8080".parse().unwrap();

        assert!(!cache.remove(&peer_id, &addr));
    }

    #[test]
    fn remove_removes_address_provided_in_param() {
        let mut cache = PeerAddresses::default();
        let peer_id = PeerId::random();
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/8080".parse().unwrap();
        let addr2: Multiaddr = "/ip4/127.0.0.1/tcp/8081".parse().unwrap();
        let addr3: Multiaddr = "/ip4/127.0.0.1/tcp/8082".parse().unwrap();

        cache.add(peer_id, addr.clone());
        cache.add(peer_id, addr2.clone());
        cache.add(peer_id, addr3.clone());

        assert!(cache.remove(&peer_id, &addr2));

        let mut cached = cache.get(&peer_id).collect::<Vec<Multiaddr>>();
        cached.sort();

        let expected = prepare_expected_addrs(peer_id, [addr, addr3].into_iter());

        assert_eq!(cached, expected);
    }

    #[test]
    fn add_adds_new_address_to_cache() {
        let mut cache = PeerAddresses::default();
        let peer_id = PeerId::random();
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/8080".parse().unwrap();

        assert!(cache.add(peer_id, addr.clone()));

        let mut cached = cache.get(&peer_id).collect::<Vec<Multiaddr>>();
        cached.sort();
        let expected = prepare_expected_addrs(peer_id, [addr].into_iter());

        assert_eq!(cached, expected);
    }

    #[test]
    fn add_adds_address_to_cache_to_existing_key() {
        let mut cache = PeerAddresses::default();
        let peer_id = PeerId::random();
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/8080".parse().unwrap();
        let addr2: Multiaddr = "/ip4/127.0.0.1/tcp/8081".parse().unwrap();
        let addr3: Multiaddr = "/ip4/127.0.0.1/tcp/8082".parse().unwrap();

        assert!(cache.add(peer_id, addr.clone()));

        cache.add(peer_id, addr2.clone());
        cache.add(peer_id, addr3.clone());

        let expected = prepare_expected_addrs(peer_id, [addr, addr2, addr3].into_iter());

        let mut cached = cache.get(&peer_id).collect::<Vec<Multiaddr>>();
        cached.sort();

        assert_eq!(cached, expected);
    }

    fn prepare_expected_addrs(
        peer_id: PeerId,
        addrs: impl Iterator<Item = Multiaddr>,
    ) -> Vec<Multiaddr> {
        let mut addrs = addrs
            .filter_map(|a| a.with_p2p(peer_id).ok())
            .collect::<Vec<Multiaddr>>();
        addrs.sort();
        addrs
    }

    fn new_external_addr_of_peer1(peer_id: PeerId) -> FromSwarm<'static> {
        FromSwarm::NewExternalAddrOfPeer(NewExternalAddrOfPeer {
            peer_id,
            addr: &MEMORY_ADDR_1000,
        })
    }

    fn new_external_addr_of_peer2(peer_id: PeerId) -> FromSwarm<'static> {
        FromSwarm::NewExternalAddrOfPeer(NewExternalAddrOfPeer {
            peer_id,
            addr: &MEMORY_ADDR_2000,
        })
    }

    fn prepare_errors(addrs: Vec<Multiaddr>) -> Vec<(Multiaddr, TransportError<io::Error>)> {
        let errors: Vec<(Multiaddr, TransportError<io::Error>)> = addrs
            .iter()
            .map(|addr| {
                (
                    addr.clone(),
                    TransportError::Other(io::Error::new(
                        io::ErrorKind::Other,
                        MemoryTransportError::Unreachable,
                    )),
                )
            })
            .collect();
        errors
    }

    static MEMORY_ADDR_1000: Lazy<Multiaddr> =
        Lazy::new(|| Multiaddr::empty().with(Protocol::Memory(1000)));
    static MEMORY_ADDR_2000: Lazy<Multiaddr> =
        Lazy::new(|| Multiaddr::empty().with(Protocol::Memory(2000)));
}
