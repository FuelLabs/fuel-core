// Copyright 2019 Parity Technologies (UK) Ltd.
// Copyright 2021 Protocol Labs.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

use crate::ConnectionId;
use libp2p_core::connection::Endpoint;
use libp2p_core::multiaddr::Protocol;
use libp2p_core::transport::PortUse;
use libp2p_core::Multiaddr;
use libp2p_identity::PeerId;
use std::num::NonZeroU8;

macro_rules! fn_override_role {
    () => {
        /// Override role of local node on connection. I.e. execute the dial _as a
        /// listener_.
        ///
        /// See
        /// [`ConnectedPoint::Dialer`](libp2p_core::connection::ConnectedPoint::Dialer)
        /// for details.
        pub fn override_role(mut self) -> Self {
            self.role_override = Endpoint::Listener;
            self
        }
    };
}

macro_rules! fn_allocate_new_port {
    () => {
        /// Enforce the allocation of a new port.
        /// Default behaviour is best effort reuse of existing ports. If there is no existing
        /// fitting listener, a new port is allocated.
        pub fn allocate_new_port(mut self) -> Self {
            self.port_use = PortUse::New;
            self
        }
    };
}

/// Options to configure a dial to a known or unknown peer.
///
/// Used in [`Swarm::dial`](crate::Swarm::dial) and
/// [`ToSwarm::Dial`](crate::behaviour::ToSwarm::Dial).
///
/// To construct use either of:
///
/// - [`DialOpts::peer_id`] dialing a known peer
///
/// - [`DialOpts::unknown_peer_id`] dialing an unknown peer
#[derive(Debug)]
pub struct DialOpts {
    peer_id: Option<PeerId>,
    condition: PeerCondition,
    addresses: Vec<Multiaddr>,
    extend_addresses_through_behaviour: bool,
    role_override: Endpoint,
    dial_concurrency_factor_override: Option<NonZeroU8>,
    connection_id: ConnectionId,
    port_use: PortUse,
}

impl DialOpts {
    /// Dial a known peer.
    ///
    ///   ```
    ///   # use libp2p_swarm::dial_opts::{DialOpts, PeerCondition};
    ///   # use libp2p_identity::PeerId;
    ///   DialOpts::peer_id(PeerId::random())
    ///      .condition(PeerCondition::Disconnected)
    ///      .addresses(vec!["/ip6/::1/tcp/12345".parse().unwrap()])
    ///      .extend_addresses_through_behaviour()
    ///      .build();
    ///   ```
    pub fn peer_id(peer_id: PeerId) -> WithPeerId {
        WithPeerId {
            peer_id,
            condition: Default::default(),
            role_override: Endpoint::Dialer,
            dial_concurrency_factor_override: Default::default(),
            port_use: PortUse::Reuse,
        }
    }

    /// Dial an unknown peer.
    ///
    ///   ```
    ///   # use libp2p_swarm::dial_opts::DialOpts;
    ///   DialOpts::unknown_peer_id()
    ///      .address("/ip6/::1/tcp/12345".parse().unwrap())
    ///      .build();
    ///   ```
    pub fn unknown_peer_id() -> WithoutPeerId {
        WithoutPeerId {}
    }

    /// Retrieves the [`PeerId`] from the [`DialOpts`] if specified or otherwise tries to extract it
    /// from the multihash in the `/p2p` part of the address, if present.
    pub fn get_peer_id(&self) -> Option<PeerId> {
        if let Some(peer_id) = self.peer_id {
            return Some(peer_id);
        }

        let first_address = self.addresses.first()?;
        let last_protocol = first_address.iter().last()?;

        if let Protocol::P2p(p) = last_protocol {
            return Some(p);
        }

        None
    }

    /// Get the [`ConnectionId`] of this dial attempt.
    ///
    /// All future events of this dial will be associated with this ID.
    /// See [`DialFailure`](crate::DialFailure) and [`ConnectionEstablished`](crate::behaviour::ConnectionEstablished).
    pub fn connection_id(&self) -> ConnectionId {
        self.connection_id
    }

    pub(crate) fn get_addresses(&self) -> Vec<Multiaddr> {
        self.addresses.clone()
    }

    pub(crate) fn extend_addresses_through_behaviour(&self) -> bool {
        self.extend_addresses_through_behaviour
    }

    pub(crate) fn peer_condition(&self) -> PeerCondition {
        self.condition
    }

    pub(crate) fn dial_concurrency_override(&self) -> Option<NonZeroU8> {
        self.dial_concurrency_factor_override
    }

    pub(crate) fn role_override(&self) -> Endpoint {
        self.role_override
    }

    pub(crate) fn port_use(&self) -> PortUse {
        self.port_use
    }
}

impl From<Multiaddr> for DialOpts {
    fn from(address: Multiaddr) -> Self {
        DialOpts::unknown_peer_id().address(address).build()
    }
}

impl From<PeerId> for DialOpts {
    fn from(peer_id: PeerId) -> Self {
        DialOpts::peer_id(peer_id).build()
    }
}

#[derive(Debug)]
pub struct WithPeerId {
    peer_id: PeerId,
    condition: PeerCondition,
    role_override: Endpoint,
    dial_concurrency_factor_override: Option<NonZeroU8>,
    port_use: PortUse,
}

impl WithPeerId {
    /// Specify a [`PeerCondition`] for the dial.
    pub fn condition(mut self, condition: PeerCondition) -> Self {
        self.condition = condition;
        self
    }

    /// Override
    /// Number of addresses concurrently dialed for a single outbound connection attempt.
    pub fn override_dial_concurrency_factor(mut self, factor: NonZeroU8) -> Self {
        self.dial_concurrency_factor_override = Some(factor);
        self
    }

    /// Specify a set of addresses to be used to dial the known peer.
    pub fn addresses(self, addresses: Vec<Multiaddr>) -> WithPeerIdWithAddresses {
        WithPeerIdWithAddresses {
            peer_id: self.peer_id,
            condition: self.condition,
            addresses,
            extend_addresses_through_behaviour: false,
            role_override: self.role_override,
            dial_concurrency_factor_override: self.dial_concurrency_factor_override,
            port_use: self.port_use,
        }
    }

    fn_override_role!();
    fn_allocate_new_port!();

    /// Build the final [`DialOpts`].
    pub fn build(self) -> DialOpts {
        DialOpts {
            peer_id: Some(self.peer_id),
            condition: self.condition,
            addresses: vec![],
            extend_addresses_through_behaviour: true,
            role_override: self.role_override,
            dial_concurrency_factor_override: self.dial_concurrency_factor_override,
            connection_id: ConnectionId::next(),
            port_use: self.port_use,
        }
    }
}

#[derive(Debug)]
pub struct WithPeerIdWithAddresses {
    peer_id: PeerId,
    condition: PeerCondition,
    addresses: Vec<Multiaddr>,
    extend_addresses_through_behaviour: bool,
    role_override: Endpoint,
    dial_concurrency_factor_override: Option<NonZeroU8>,
    port_use: PortUse,
}

impl WithPeerIdWithAddresses {
    /// Specify a [`PeerCondition`] for the dial.
    pub fn condition(mut self, condition: PeerCondition) -> Self {
        self.condition = condition;
        self
    }

    /// In addition to the provided addresses, extend the set via
    /// [`NetworkBehaviour::handle_pending_outbound_connection`](crate::behaviour::NetworkBehaviour::handle_pending_outbound_connection).
    pub fn extend_addresses_through_behaviour(mut self) -> Self {
        self.extend_addresses_through_behaviour = true;
        self
    }

    fn_override_role!();
    fn_allocate_new_port!();

    /// Override
    /// Number of addresses concurrently dialed for a single outbound connection attempt.
    pub fn override_dial_concurrency_factor(mut self, factor: NonZeroU8) -> Self {
        self.dial_concurrency_factor_override = Some(factor);
        self
    }

    /// Build the final [`DialOpts`].
    pub fn build(self) -> DialOpts {
        DialOpts {
            peer_id: Some(self.peer_id),
            condition: self.condition,
            addresses: self.addresses,
            extend_addresses_through_behaviour: self.extend_addresses_through_behaviour,
            role_override: self.role_override,
            dial_concurrency_factor_override: self.dial_concurrency_factor_override,
            connection_id: ConnectionId::next(),
            port_use: self.port_use,
        }
    }
}

#[derive(Debug)]
pub struct WithoutPeerId {}

impl WithoutPeerId {
    /// Specify a single address to dial the unknown peer.
    pub fn address(self, address: Multiaddr) -> WithoutPeerIdWithAddress {
        WithoutPeerIdWithAddress {
            address,
            role_override: Endpoint::Dialer,
            port_use: PortUse::Reuse,
        }
    }
}

#[derive(Debug)]
pub struct WithoutPeerIdWithAddress {
    address: Multiaddr,
    role_override: Endpoint,
    port_use: PortUse,
}

impl WithoutPeerIdWithAddress {
    fn_override_role!();
    fn_allocate_new_port!();

    /// Build the final [`DialOpts`].
    pub fn build(self) -> DialOpts {
        DialOpts {
            peer_id: None,
            condition: PeerCondition::Always,
            addresses: vec![self.address],
            extend_addresses_through_behaviour: false,
            role_override: self.role_override,
            dial_concurrency_factor_override: None,
            connection_id: ConnectionId::next(),
            port_use: self.port_use,
        }
    }
}

/// The available conditions under which a new dialing attempt to
/// a known peer is initiated.
///
/// ```
/// # use libp2p_swarm::dial_opts::{DialOpts, PeerCondition};
/// # use libp2p_identity::PeerId;
/// #
/// DialOpts::peer_id(PeerId::random())
///    .condition(PeerCondition::Disconnected)
///    .build();
/// ```
#[derive(Debug, Copy, Clone, Default)]
pub enum PeerCondition {
    /// A new dialing attempt is initiated _only if_ the peer is currently
    /// considered disconnected, i.e. there is no established connection.
    Disconnected,
    /// A new dialing attempt is initiated _only if_ there is currently
    /// no ongoing dialing attempt, i.e. the peer is either considered
    /// disconnected or connected but without an ongoing dialing attempt.
    NotDialing,
    /// A combination of [`Disconnected`](PeerCondition::Disconnected) and
    /// [`NotDialing`](PeerCondition::NotDialing). A new dialing attempt is
    /// iniated _only if_ the peer is both considered disconnected and there
    /// is currently no ongoing dialing attempt.
    #[default]
    DisconnectedAndNotDialing,
    /// A new dialing attempt is always initiated, only subject to the
    /// configured connection limits.
    Always,
}
