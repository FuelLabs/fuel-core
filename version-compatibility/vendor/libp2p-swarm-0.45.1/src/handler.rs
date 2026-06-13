// Copyright 2018 Parity Technologies (UK) Ltd.
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

//! Once a connection to a remote peer is established, a [`ConnectionHandler`] negotiates
//! and handles one or more specific protocols on the connection.
//!
//! Protocols are negotiated and used on individual substreams of the connection. Thus a
//! [`ConnectionHandler`] defines the inbound and outbound upgrades to apply when creating a new
//! inbound or outbound substream, respectively, and is notified by a [`Swarm`](crate::Swarm) when
//! these upgrades have been successfully applied, including the final output of the upgrade. A
//! [`ConnectionHandler`] can then continue communicating with the peer over the substream using the
//! negotiated protocol(s).
//!
//! Two [`ConnectionHandler`]s can be composed with [`ConnectionHandler::select()`]
//! in order to build a new handler supporting the combined set of protocols,
//! with methods being dispatched to the appropriate handler according to the
//! used protocol(s) determined by the associated types of the handlers.
//!
//! > **Note**: A [`ConnectionHandler`] handles one or more protocols in the context of a single
//! >           connection with a remote. In order to handle a protocol that requires knowledge of
//! >           the network as a whole, see the
//! >           [`NetworkBehaviour`](crate::behaviour::NetworkBehaviour) trait.

pub mod either;
mod map_in;
mod map_out;
pub mod multi;
mod one_shot;
mod pending;
mod select;

use crate::connection::AsStrHashEq;
pub use crate::upgrade::{InboundUpgradeSend, OutboundUpgradeSend, SendWrapper, UpgradeInfoSend};
pub use map_in::MapInEvent;
pub use map_out::MapOutEvent;
pub use one_shot::{OneShotHandler, OneShotHandlerConfig};
pub use pending::PendingConnectionHandler;
pub use select::ConnectionHandlerSelect;
use smallvec::SmallVec;

use crate::StreamProtocol;
use core::slice;
use libp2p_core::Multiaddr;
use std::collections::{HashMap, HashSet};
use std::{error, fmt, io, task::Context, task::Poll, time::Duration};

/// A handler for a set of protocols used on a connection with a remote.
///
/// This trait should be implemented for a type that maintains the state for
/// the execution of a specific protocol with a remote.
///
/// # Handling a protocol
///
/// Communication with a remote over a set of protocols is initiated in one of two ways:
///
///   1. Dialing by initiating a new outbound substream. In order to do so,
///      [`ConnectionHandler::poll()`] must return an [`ConnectionHandlerEvent::OutboundSubstreamRequest`],
///      providing an instance of [`libp2p_core::upgrade::OutboundUpgrade`] that is used to negotiate the
///      protocol(s). Upon success, [`ConnectionHandler::on_connection_event`] is called with
///      [`ConnectionEvent::FullyNegotiatedOutbound`] translating the final output of the upgrade.
///
///   2. Listening by accepting a new inbound substream. When a new inbound substream
///      is created on a connection, [`ConnectionHandler::listen_protocol`] is called
///      to obtain an instance of [`libp2p_core::upgrade::InboundUpgrade`] that is used to
///      negotiate the protocol(s). Upon success,
///      [`ConnectionHandler::on_connection_event`] is called with [`ConnectionEvent::FullyNegotiatedInbound`]
///      translating the final output of the upgrade.
///
///
/// # Connection Keep-Alive
///
/// A [`ConnectionHandler`] can influence the lifetime of the underlying connection
/// through [`ConnectionHandler::connection_keep_alive`]. That is, the protocol
/// implemented by the handler can include conditions for terminating the connection.
/// The lifetime of successfully negotiated substreams is fully controlled by the handler.
///
/// Implementors of this trait should keep in mind that the connection can be closed at any time.
/// When a connection is closed gracefully, the substreams used by the handler may still
/// continue reading data until the remote closes its side of the connection.
pub trait ConnectionHandler: Send + 'static {
    /// A type representing the message(s) a [`NetworkBehaviour`](crate::behaviour::NetworkBehaviour) can send to a [`ConnectionHandler`] via [`ToSwarm::NotifyHandler`](crate::behaviour::ToSwarm::NotifyHandler)
    type FromBehaviour: fmt::Debug + Send + 'static;
    /// A type representing message(s) a [`ConnectionHandler`] can send to a [`NetworkBehaviour`](crate::behaviour::NetworkBehaviour) via [`ConnectionHandlerEvent::NotifyBehaviour`].
    type ToBehaviour: fmt::Debug + Send + 'static;
    /// The inbound upgrade for the protocol(s) used by the handler.
    type InboundProtocol: InboundUpgradeSend;
    /// The outbound upgrade for the protocol(s) used by the handler.
    type OutboundProtocol: OutboundUpgradeSend;
    /// The type of additional information returned from `listen_protocol`.
    type InboundOpenInfo: Send + 'static;
    /// The type of additional information passed to an `OutboundSubstreamRequest`.
    type OutboundOpenInfo: Send + 'static;

    /// The [`InboundUpgrade`](libp2p_core::upgrade::InboundUpgrade) to apply on inbound
    /// substreams to negotiate the desired protocols.
    ///
    /// > **Note**: The returned `InboundUpgrade` should always accept all the generally
    /// >           supported protocols, even if in a specific context a particular one is
    /// >           not supported, (eg. when only allowing one substream at a time for a protocol).
    /// >           This allows a remote to put the list of supported protocols in a cache.
    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo>;

    /// Returns whether the connection should be kept alive.
    ///
    /// ## Keep alive algorithm
    ///
    /// A connection is always kept alive:
    ///
    /// - Whilst a [`ConnectionHandler`] returns [`Poll::Ready`].
    /// - We are negotiating inbound or outbound streams.
    /// - There are active [`Stream`](crate::Stream)s on the connection.
    ///
    /// The combination of the above means that _most_ protocols will not need to override this method.
    /// This method is only invoked when all of the above are `false`, i.e. when the connection is entirely idle.
    ///
    /// ## Exceptions
    ///
    /// - Protocols like [circuit-relay v2](https://github.com/libp2p/specs/blob/master/relay/circuit-v2.md) need to keep a connection alive beyond these circumstances and can thus override this method.
    /// - Protocols like [ping](https://github.com/libp2p/specs/blob/master/ping/ping.md) **don't** want to keep a connection alive despite an active streams.
    ///
    /// In that case, protocol authors can use [`Stream::ignore_for_keep_alive`](crate::Stream::ignore_for_keep_alive) to opt-out a particular stream from the keep-alive algorithm.
    fn connection_keep_alive(&self) -> bool {
        false
    }

    /// Should behave like `Stream::poll()`.
    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<Self::OutboundProtocol, Self::OutboundOpenInfo, Self::ToBehaviour>,
    >;

    /// Gracefully close the [`ConnectionHandler`].
    ///
    /// The contract for this function is equivalent to a [`Stream`](futures::Stream).
    /// When a connection is being shut down, we will first poll this function to completion.
    /// Following that, the physical connection will be shut down.
    ///
    /// This is also called when the shutdown was initiated due to an error on the connection.
    /// We therefore cannot guarantee that performing IO within here will succeed.
    ///
    /// To signal completion, [`Poll::Ready(None)`] should be returned.
    ///
    /// Implementations MUST have a [`fuse`](futures::StreamExt::fuse)-like behaviour.
    /// That is, [`Poll::Ready(None)`] MUST be returned on repeated calls to [`ConnectionHandler::poll_close`].
    fn poll_close(&mut self, _: &mut Context<'_>) -> Poll<Option<Self::ToBehaviour>> {
        Poll::Ready(None)
    }

    /// Adds a closure that turns the input event into something else.
    fn map_in_event<TNewIn, TMap>(self, map: TMap) -> MapInEvent<Self, TNewIn, TMap>
    where
        Self: Sized,
        TMap: Fn(&TNewIn) -> Option<&Self::FromBehaviour>,
    {
        MapInEvent::new(self, map)
    }

    /// Adds a closure that turns the output event into something else.
    fn map_out_event<TMap, TNewOut>(self, map: TMap) -> MapOutEvent<Self, TMap>
    where
        Self: Sized,
        TMap: FnMut(Self::ToBehaviour) -> TNewOut,
    {
        MapOutEvent::new(self, map)
    }

    /// Creates a new [`ConnectionHandler`] that selects either this handler or
    /// `other` by delegating methods calls appropriately.
    fn select<TProto2>(self, other: TProto2) -> ConnectionHandlerSelect<Self, TProto2>
    where
        Self: Sized,
    {
        ConnectionHandlerSelect::new(self, other)
    }

    /// Informs the handler about an event from the [`NetworkBehaviour`](super::NetworkBehaviour).
    fn on_behaviour_event(&mut self, _event: Self::FromBehaviour);

    fn on_connection_event(
        &mut self,
        event: ConnectionEvent<
            Self::InboundProtocol,
            Self::OutboundProtocol,
            Self::InboundOpenInfo,
            Self::OutboundOpenInfo,
        >,
    );
}

/// Enumeration with the list of the possible stream events
/// to pass to [`on_connection_event`](ConnectionHandler::on_connection_event).
#[non_exhaustive]
pub enum ConnectionEvent<'a, IP: InboundUpgradeSend, OP: OutboundUpgradeSend, IOI, OOI> {
    /// Informs the handler about the output of a successful upgrade on a new inbound substream.
    FullyNegotiatedInbound(FullyNegotiatedInbound<IP, IOI>),
    /// Informs the handler about the output of a successful upgrade on a new outbound stream.
    FullyNegotiatedOutbound(FullyNegotiatedOutbound<OP, OOI>),
    /// Informs the handler about a change in the address of the remote.
    AddressChange(AddressChange<'a>),
    /// Informs the handler that upgrading an outbound substream to the given protocol has failed.
    DialUpgradeError(DialUpgradeError<OOI, OP>),
    /// Informs the handler that upgrading an inbound substream to the given protocol has failed.
    ListenUpgradeError(ListenUpgradeError<IOI, IP>),
    /// The local [`ConnectionHandler`] added or removed support for one or more protocols.
    LocalProtocolsChange(ProtocolsChange<'a>),
    /// The remote [`ConnectionHandler`] now supports a different set of protocols.
    RemoteProtocolsChange(ProtocolsChange<'a>),
}

impl<'a, IP, OP, IOI, OOI> fmt::Debug for ConnectionEvent<'a, IP, OP, IOI, OOI>
where
    IP: InboundUpgradeSend + fmt::Debug,
    IP::Output: fmt::Debug,
    IP::Error: fmt::Debug,
    OP: OutboundUpgradeSend + fmt::Debug,
    OP::Output: fmt::Debug,
    OP::Error: fmt::Debug,
    IOI: fmt::Debug,
    OOI: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectionEvent::FullyNegotiatedInbound(v) => {
                f.debug_tuple("FullyNegotiatedInbound").field(v).finish()
            }
            ConnectionEvent::FullyNegotiatedOutbound(v) => {
                f.debug_tuple("FullyNegotiatedOutbound").field(v).finish()
            }
            ConnectionEvent::AddressChange(v) => f.debug_tuple("AddressChange").field(v).finish(),
            ConnectionEvent::DialUpgradeError(v) => {
                f.debug_tuple("DialUpgradeError").field(v).finish()
            }
            ConnectionEvent::ListenUpgradeError(v) => {
                f.debug_tuple("ListenUpgradeError").field(v).finish()
            }
            ConnectionEvent::LocalProtocolsChange(v) => {
                f.debug_tuple("LocalProtocolsChange").field(v).finish()
            }
            ConnectionEvent::RemoteProtocolsChange(v) => {
                f.debug_tuple("RemoteProtocolsChange").field(v).finish()
            }
        }
    }
}

impl<'a, IP: InboundUpgradeSend, OP: OutboundUpgradeSend, IOI, OOI>
    ConnectionEvent<'a, IP, OP, IOI, OOI>
{
    /// Whether the event concerns an outbound stream.
    pub fn is_outbound(&self) -> bool {
        match self {
            ConnectionEvent::DialUpgradeError(_) | ConnectionEvent::FullyNegotiatedOutbound(_) => {
                true
            }
            ConnectionEvent::FullyNegotiatedInbound(_)
            | ConnectionEvent::AddressChange(_)
            | ConnectionEvent::LocalProtocolsChange(_)
            | ConnectionEvent::RemoteProtocolsChange(_)
            | ConnectionEvent::ListenUpgradeError(_) => false,
        }
    }

    /// Whether the event concerns an inbound stream.
    pub fn is_inbound(&self) -> bool {
        match self {
            ConnectionEvent::FullyNegotiatedInbound(_) | ConnectionEvent::ListenUpgradeError(_) => {
                true
            }
            ConnectionEvent::FullyNegotiatedOutbound(_)
            | ConnectionEvent::AddressChange(_)
            | ConnectionEvent::LocalProtocolsChange(_)
            | ConnectionEvent::RemoteProtocolsChange(_)
            | ConnectionEvent::DialUpgradeError(_) => false,
        }
    }
}

/// [`ConnectionEvent`] variant that informs the handler about
/// the output of a successful upgrade on a new inbound substream.
///
/// Note that it is up to the [`ConnectionHandler`] implementation to manage the lifetime of the
/// negotiated inbound substreams. E.g. the implementation has to enforce a limit on the number
/// of simultaneously open negotiated inbound substreams. In other words it is up to the
/// [`ConnectionHandler`] implementation to stop a malicious remote node to open and keep alive
/// an excessive amount of inbound substreams.
#[derive(Debug)]
pub struct FullyNegotiatedInbound<IP: InboundUpgradeSend, IOI> {
    pub protocol: IP::Output,
    pub info: IOI,
}

/// [`ConnectionEvent`] variant that informs the handler about successful upgrade on a new outbound stream.
///
/// The `protocol` field is the information that was previously passed to
/// [`ConnectionHandlerEvent::OutboundSubstreamRequest`].
#[derive(Debug)]
pub struct FullyNegotiatedOutbound<OP: OutboundUpgradeSend, OOI> {
    pub protocol: OP::Output,
    pub info: OOI,
}

/// [`ConnectionEvent`] variant that informs the handler about a change in the address of the remote.
#[derive(Debug)]
pub struct AddressChange<'a> {
    pub new_address: &'a Multiaddr,
}

/// [`ConnectionEvent`] variant that informs the handler about a change in the protocols supported on the connection.
#[derive(Debug, Clone)]
pub enum ProtocolsChange<'a> {
    Added(ProtocolsAdded<'a>),
    Removed(ProtocolsRemoved<'a>),
}

impl<'a> ProtocolsChange<'a> {
    /// Compute the protocol change for the initial set of protocols.
    pub(crate) fn from_initial_protocols<'b, T: AsRef<str> + 'b>(
        new_protocols: impl IntoIterator<Item = &'b T>,
        buffer: &'a mut Vec<StreamProtocol>,
    ) -> Self {
        buffer.clear();
        buffer.extend(
            new_protocols
                .into_iter()
                .filter_map(|i| StreamProtocol::try_from_owned(i.as_ref().to_owned()).ok()),
        );

        ProtocolsChange::Added(ProtocolsAdded {
            protocols: buffer.iter(),
        })
    }

    /// Compute the [`ProtocolsChange`] that results from adding `to_add` to `existing_protocols`.
    ///
    /// Returns `None` if the change is a no-op, i.e. `to_add` is a subset of `existing_protocols`.
    pub(crate) fn add(
        existing_protocols: &HashSet<StreamProtocol>,
        to_add: HashSet<StreamProtocol>,
        buffer: &'a mut Vec<StreamProtocol>,
    ) -> Option<Self> {
        buffer.clear();
        buffer.extend(
            to_add
                .into_iter()
                .filter(|i| !existing_protocols.contains(i)),
        );

        if buffer.is_empty() {
            return None;
        }

        Some(Self::Added(ProtocolsAdded {
            protocols: buffer.iter(),
        }))
    }

    /// Compute the [`ProtocolsChange`] that results from removing `to_remove` from `existing_protocols`. Removes the protocols from `existing_protocols`.
    ///
    /// Returns `None` if the change is a no-op, i.e. none of the protocols in `to_remove` are in `existing_protocols`.
    pub(crate) fn remove(
        existing_protocols: &mut HashSet<StreamProtocol>,
        to_remove: HashSet<StreamProtocol>,
        buffer: &'a mut Vec<StreamProtocol>,
    ) -> Option<Self> {
        buffer.clear();
        buffer.extend(
            to_remove
                .into_iter()
                .filter_map(|i| existing_protocols.take(&i)),
        );

        if buffer.is_empty() {
            return None;
        }

        Some(Self::Removed(ProtocolsRemoved {
            protocols: buffer.iter(),
        }))
    }

    /// Compute the [`ProtocolsChange`]s required to go from `existing_protocols` to `new_protocols`.
    pub(crate) fn from_full_sets<T: AsRef<str>>(
        existing_protocols: &mut HashMap<AsStrHashEq<T>, bool>,
        new_protocols: impl IntoIterator<Item = T>,
        buffer: &'a mut Vec<StreamProtocol>,
    ) -> SmallVec<[Self; 2]> {
        buffer.clear();

        // Initially, set the boolean for all protocols to `false`, meaning "not visited".
        for v in existing_protocols.values_mut() {
            *v = false;
        }

        let mut new_protocol_count = 0; // We can only iterate `new_protocols` once, so keep track of its length separately.
        for new_protocol in new_protocols {
            existing_protocols
                .entry(AsStrHashEq(new_protocol))
                .and_modify(|v| *v = true) // Mark protocol as visited (i.e. we still support it)
                .or_insert_with_key(|k| {
                    // Encountered a previously unsupported protocol, remember it in `buffer`.
                    buffer.extend(StreamProtocol::try_from_owned(k.0.as_ref().to_owned()).ok());
                    true
                });
            new_protocol_count += 1;
        }

        if new_protocol_count == existing_protocols.len() && buffer.is_empty() {
            return SmallVec::new();
        }

        let num_new_protocols = buffer.len();
        // Drain all protocols that we haven't visited.
        // For existing protocols that are not in `new_protocols`, the boolean will be false, meaning we need to remove it.
        existing_protocols.retain(|p, &mut is_supported| {
            if !is_supported {
                buffer.extend(StreamProtocol::try_from_owned(p.0.as_ref().to_owned()).ok());
            }

            is_supported
        });

        let (added, removed) = buffer.split_at(num_new_protocols);
        let mut changes = SmallVec::new();
        if !added.is_empty() {
            changes.push(ProtocolsChange::Added(ProtocolsAdded {
                protocols: added.iter(),
            }));
        }
        if !removed.is_empty() {
            changes.push(ProtocolsChange::Removed(ProtocolsRemoved {
                protocols: removed.iter(),
            }));
        }
        changes
    }
}

/// An [`Iterator`] over all protocols that have been added.
#[derive(Debug, Clone)]
pub struct ProtocolsAdded<'a> {
    pub(crate) protocols: slice::Iter<'a, StreamProtocol>,
}

/// An [`Iterator`] over all protocols that have been removed.
#[derive(Debug, Clone)]
pub struct ProtocolsRemoved<'a> {
    pub(crate) protocols: slice::Iter<'a, StreamProtocol>,
}

impl<'a> Iterator for ProtocolsAdded<'a> {
    type Item = &'a StreamProtocol;
    fn next(&mut self) -> Option<Self::Item> {
        self.protocols.next()
    }
}

impl<'a> Iterator for ProtocolsRemoved<'a> {
    type Item = &'a StreamProtocol;
    fn next(&mut self) -> Option<Self::Item> {
        self.protocols.next()
    }
}

/// [`ConnectionEvent`] variant that informs the handler
/// that upgrading an outbound substream to the given protocol has failed.
#[derive(Debug)]
pub struct DialUpgradeError<OOI, OP: OutboundUpgradeSend> {
    pub info: OOI,
    pub error: StreamUpgradeError<OP::Error>,
}

/// [`ConnectionEvent`] variant that informs the handler
/// that upgrading an inbound substream to the given protocol has failed.
#[derive(Debug)]
pub struct ListenUpgradeError<IOI, IP: InboundUpgradeSend> {
    pub info: IOI,
    pub error: IP::Error,
}

/// Configuration of inbound or outbound substream protocol(s)
/// for a [`ConnectionHandler`].
///
/// The inbound substream protocol(s) are defined by [`ConnectionHandler::listen_protocol`]
/// and the outbound substream protocol(s) by [`ConnectionHandlerEvent::OutboundSubstreamRequest`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct SubstreamProtocol<TUpgrade, TInfo> {
    upgrade: TUpgrade,
    info: TInfo,
    timeout: Duration,
}

impl<TUpgrade, TInfo> SubstreamProtocol<TUpgrade, TInfo> {
    /// Create a new `SubstreamProtocol` from the given upgrade.
    ///
    /// The default timeout for applying the given upgrade on a substream is
    /// 10 seconds.
    pub fn new(upgrade: TUpgrade, info: TInfo) -> Self {
        SubstreamProtocol {
            upgrade,
            info,
            timeout: Duration::from_secs(10),
        }
    }

    /// Maps a function over the protocol upgrade.
    pub fn map_upgrade<U, F>(self, f: F) -> SubstreamProtocol<U, TInfo>
    where
        F: FnOnce(TUpgrade) -> U,
    {
        SubstreamProtocol {
            upgrade: f(self.upgrade),
            info: self.info,
            timeout: self.timeout,
        }
    }

    /// Maps a function over the protocol info.
    pub fn map_info<U, F>(self, f: F) -> SubstreamProtocol<TUpgrade, U>
    where
        F: FnOnce(TInfo) -> U,
    {
        SubstreamProtocol {
            upgrade: self.upgrade,
            info: f(self.info),
            timeout: self.timeout,
        }
    }

    /// Sets a new timeout for the protocol upgrade.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Borrows the contained protocol upgrade.
    pub fn upgrade(&self) -> &TUpgrade {
        &self.upgrade
    }

    /// Borrows the contained protocol info.
    pub fn info(&self) -> &TInfo {
        &self.info
    }

    /// Borrows the timeout for the protocol upgrade.
    pub fn timeout(&self) -> &Duration {
        &self.timeout
    }

    /// Converts the substream protocol configuration into the contained upgrade.
    pub fn into_upgrade(self) -> (TUpgrade, TInfo) {
        (self.upgrade, self.info)
    }
}

/// Event produced by a handler.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConnectionHandlerEvent<TConnectionUpgrade, TOutboundOpenInfo, TCustom> {
    /// Request a new outbound substream to be opened with the remote.
    OutboundSubstreamRequest {
        /// The protocol(s) to apply on the substream.
        protocol: SubstreamProtocol<TConnectionUpgrade, TOutboundOpenInfo>,
    },
    /// We learned something about the protocols supported by the remote.
    ReportRemoteProtocols(ProtocolSupport),

    /// Event that is sent to a [`NetworkBehaviour`](crate::behaviour::NetworkBehaviour).
    NotifyBehaviour(TCustom),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolSupport {
    /// The remote now supports these additional protocols.
    Added(HashSet<StreamProtocol>),
    /// The remote no longer supports these protocols.
    Removed(HashSet<StreamProtocol>),
}

/// Event produced by a handler.
impl<TConnectionUpgrade, TOutboundOpenInfo, TCustom>
    ConnectionHandlerEvent<TConnectionUpgrade, TOutboundOpenInfo, TCustom>
{
    /// If this is an `OutboundSubstreamRequest`, maps the `info` member from a
    /// `TOutboundOpenInfo` to something else.
    pub fn map_outbound_open_info<F, I>(
        self,
        map: F,
    ) -> ConnectionHandlerEvent<TConnectionUpgrade, I, TCustom>
    where
        F: FnOnce(TOutboundOpenInfo) -> I,
    {
        match self {
            ConnectionHandlerEvent::OutboundSubstreamRequest { protocol } => {
                ConnectionHandlerEvent::OutboundSubstreamRequest {
                    protocol: protocol.map_info(map),
                }
            }
            ConnectionHandlerEvent::NotifyBehaviour(val) => {
                ConnectionHandlerEvent::NotifyBehaviour(val)
            }
            ConnectionHandlerEvent::ReportRemoteProtocols(support) => {
                ConnectionHandlerEvent::ReportRemoteProtocols(support)
            }
        }
    }

    /// If this is an `OutboundSubstreamRequest`, maps the protocol (`TConnectionUpgrade`)
    /// to something else.
    pub fn map_protocol<F, I>(self, map: F) -> ConnectionHandlerEvent<I, TOutboundOpenInfo, TCustom>
    where
        F: FnOnce(TConnectionUpgrade) -> I,
    {
        match self {
            ConnectionHandlerEvent::OutboundSubstreamRequest { protocol } => {
                ConnectionHandlerEvent::OutboundSubstreamRequest {
                    protocol: protocol.map_upgrade(map),
                }
            }
            ConnectionHandlerEvent::NotifyBehaviour(val) => {
                ConnectionHandlerEvent::NotifyBehaviour(val)
            }
            ConnectionHandlerEvent::ReportRemoteProtocols(support) => {
                ConnectionHandlerEvent::ReportRemoteProtocols(support)
            }
        }
    }

    /// If this is a `Custom` event, maps the content to something else.
    pub fn map_custom<F, I>(
        self,
        map: F,
    ) -> ConnectionHandlerEvent<TConnectionUpgrade, TOutboundOpenInfo, I>
    where
        F: FnOnce(TCustom) -> I,
    {
        match self {
            ConnectionHandlerEvent::OutboundSubstreamRequest { protocol } => {
                ConnectionHandlerEvent::OutboundSubstreamRequest { protocol }
            }
            ConnectionHandlerEvent::NotifyBehaviour(val) => {
                ConnectionHandlerEvent::NotifyBehaviour(map(val))
            }
            ConnectionHandlerEvent::ReportRemoteProtocols(support) => {
                ConnectionHandlerEvent::ReportRemoteProtocols(support)
            }
        }
    }
}

/// Error that can happen on an outbound substream opening attempt.
#[derive(Debug)]
pub enum StreamUpgradeError<TUpgrErr> {
    /// The opening attempt timed out before the negotiation was fully completed.
    Timeout,
    /// The upgrade produced an error.
    Apply(TUpgrErr),
    /// No protocol could be agreed upon.
    NegotiationFailed,
    /// An IO or otherwise unrecoverable error happened.
    Io(io::Error),
}

impl<TUpgrErr> StreamUpgradeError<TUpgrErr> {
    /// Map the inner [`StreamUpgradeError`] type.
    pub fn map_upgrade_err<F, E>(self, f: F) -> StreamUpgradeError<E>
    where
        F: FnOnce(TUpgrErr) -> E,
    {
        match self {
            StreamUpgradeError::Timeout => StreamUpgradeError::Timeout,
            StreamUpgradeError::Apply(e) => StreamUpgradeError::Apply(f(e)),
            StreamUpgradeError::NegotiationFailed => StreamUpgradeError::NegotiationFailed,
            StreamUpgradeError::Io(e) => StreamUpgradeError::Io(e),
        }
    }
}

impl<TUpgrErr> fmt::Display for StreamUpgradeError<TUpgrErr>
where
    TUpgrErr: error::Error + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StreamUpgradeError::Timeout => {
                write!(f, "Timeout error while opening a substream")
            }
            StreamUpgradeError::Apply(err) => {
                write!(f, "Apply: ")?;
                crate::print_error_chain(f, err)
            }
            StreamUpgradeError::NegotiationFailed => {
                write!(f, "no protocols could be agreed upon")
            }
            StreamUpgradeError::Io(e) => {
                write!(f, "IO error: ")?;
                crate::print_error_chain(f, e)
            }
        }
    }
}

impl<TUpgrErr> error::Error for StreamUpgradeError<TUpgrErr>
where
    TUpgrErr: error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        None
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn protocol_set_of(s: &'static str) -> HashSet<StreamProtocol> {
        s.split_whitespace()
            .map(|p| StreamProtocol::try_from_owned(format!("/{p}")).unwrap())
            .collect()
    }

    fn test_remove(
        existing: &mut HashSet<StreamProtocol>,
        to_remove: HashSet<StreamProtocol>,
    ) -> HashSet<StreamProtocol> {
        ProtocolsChange::remove(existing, to_remove, &mut Vec::new())
            .into_iter()
            .flat_map(|c| match c {
                ProtocolsChange::Added(_) => panic!("unexpected added"),
                ProtocolsChange::Removed(r) => r.cloned(),
            })
            .collect::<HashSet<_>>()
    }

    #[test]
    fn test_protocol_remove_subset() {
        let mut existing = protocol_set_of("a b c");
        let to_remove = protocol_set_of("a b");

        let change = test_remove(&mut existing, to_remove);

        assert_eq!(existing, protocol_set_of("c"));
        assert_eq!(change, protocol_set_of("a b"));
    }

    #[test]
    fn test_protocol_remove_all() {
        let mut existing = protocol_set_of("a b c");
        let to_remove = protocol_set_of("a b c");

        let change = test_remove(&mut existing, to_remove);

        assert_eq!(existing, protocol_set_of(""));
        assert_eq!(change, protocol_set_of("a b c"));
    }

    #[test]
    fn test_protocol_remove_superset() {
        let mut existing = protocol_set_of("a b c");
        let to_remove = protocol_set_of("a b c d");

        let change = test_remove(&mut existing, to_remove);

        assert_eq!(existing, protocol_set_of(""));
        assert_eq!(change, protocol_set_of("a b c"));
    }

    #[test]
    fn test_protocol_remove_none() {
        let mut existing = protocol_set_of("a b c");
        let to_remove = protocol_set_of("d");

        let change = test_remove(&mut existing, to_remove);

        assert_eq!(existing, protocol_set_of("a b c"));
        assert_eq!(change, protocol_set_of(""));
    }

    #[test]
    fn test_protocol_remove_none_from_empty() {
        let mut existing = protocol_set_of("");
        let to_remove = protocol_set_of("d");

        let change = test_remove(&mut existing, to_remove);

        assert_eq!(existing, protocol_set_of(""));
        assert_eq!(change, protocol_set_of(""));
    }

    fn test_from_full_sets(
        existing: HashSet<StreamProtocol>,
        new: HashSet<StreamProtocol>,
    ) -> [HashSet<StreamProtocol>; 2] {
        let mut buffer = Vec::new();
        let mut existing = existing
            .iter()
            .map(|p| (AsStrHashEq(p.as_ref()), true))
            .collect::<HashMap<_, _>>();

        let changes = ProtocolsChange::from_full_sets(
            &mut existing,
            new.iter().map(AsRef::as_ref),
            &mut buffer,
        );

        let mut added_changes = HashSet::new();
        let mut removed_changes = HashSet::new();

        for change in changes {
            match change {
                ProtocolsChange::Added(a) => {
                    added_changes.extend(a.cloned());
                }
                ProtocolsChange::Removed(r) => {
                    removed_changes.extend(r.cloned());
                }
            }
        }

        [removed_changes, added_changes]
    }

    #[test]
    fn test_from_full_stes_subset() {
        let existing = protocol_set_of("a b c");
        let new = protocol_set_of("a b");

        let [removed_changes, added_changes] = test_from_full_sets(existing, new);

        assert_eq!(added_changes, protocol_set_of(""));
        assert_eq!(removed_changes, protocol_set_of("c"));
    }

    #[test]
    fn test_from_full_sets_superset() {
        let existing = protocol_set_of("a b");
        let new = protocol_set_of("a b c");

        let [removed_changes, added_changes] = test_from_full_sets(existing, new);

        assert_eq!(added_changes, protocol_set_of("c"));
        assert_eq!(removed_changes, protocol_set_of(""));
    }

    #[test]
    fn test_from_full_sets_intersection() {
        let existing = protocol_set_of("a b c");
        let new = protocol_set_of("b c d");

        let [removed_changes, added_changes] = test_from_full_sets(existing, new);

        assert_eq!(added_changes, protocol_set_of("d"));
        assert_eq!(removed_changes, protocol_set_of("a"));
    }

    #[test]
    fn test_from_full_sets_disjoint() {
        let existing = protocol_set_of("a b c");
        let new = protocol_set_of("d e f");

        let [removed_changes, added_changes] = test_from_full_sets(existing, new);

        assert_eq!(added_changes, protocol_set_of("d e f"));
        assert_eq!(removed_changes, protocol_set_of("a b c"));
    }

    #[test]
    fn test_from_full_sets_empty() {
        let existing = protocol_set_of("");
        let new = protocol_set_of("");

        let [removed_changes, added_changes] = test_from_full_sets(existing, new);

        assert_eq!(added_changes, protocol_set_of(""));
        assert_eq!(removed_changes, protocol_set_of(""));
    }
}
