// Copyright 2020 Parity Technologies (UK) Ltd.
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

//! A [`ConnectionHandler`] implementation that combines multiple other [`ConnectionHandler`]s
//! indexed by some key.

use crate::handler::{
    AddressChange, ConnectionEvent, ConnectionHandler, ConnectionHandlerEvent, DialUpgradeError,
    FullyNegotiatedInbound, FullyNegotiatedOutbound, ListenUpgradeError, SubstreamProtocol,
};
use crate::upgrade::{InboundUpgradeSend, OutboundUpgradeSend, UpgradeInfoSend};
use crate::Stream;
use futures::{future::BoxFuture, prelude::*, ready};
use rand::Rng;
use std::{
    cmp,
    collections::{HashMap, HashSet},
    error,
    fmt::{self, Debug},
    hash::Hash,
    iter,
    task::{Context, Poll},
    time::Duration,
};

/// A [`ConnectionHandler`] for multiple [`ConnectionHandler`]s of the same type.
#[derive(Clone)]
pub struct MultiHandler<K, H> {
    handlers: HashMap<K, H>,
}

impl<K, H> fmt::Debug for MultiHandler<K, H>
where
    K: fmt::Debug + Eq + Hash,
    H: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MultiHandler")
            .field("handlers", &self.handlers)
            .finish()
    }
}

impl<K, H> MultiHandler<K, H>
where
    K: Clone + Debug + Hash + Eq + Send + 'static,
    H: ConnectionHandler,
{
    /// Create and populate a `MultiHandler` from the given handler iterator.
    ///
    /// It is an error for any two protocols handlers to share the same protocol name.
    pub fn try_from_iter<I>(iter: I) -> Result<Self, DuplicateProtonameError>
    where
        I: IntoIterator<Item = (K, H)>,
    {
        let m = MultiHandler {
            handlers: HashMap::from_iter(iter),
        };
        uniq_proto_names(
            m.handlers
                .values()
                .map(|h| h.listen_protocol().into_upgrade().0),
        )?;
        Ok(m)
    }

    fn on_listen_upgrade_error(
        &mut self,
        ListenUpgradeError {
            error: (key, error),
            mut info,
        }: ListenUpgradeError<
            <Self as ConnectionHandler>::InboundOpenInfo,
            <Self as ConnectionHandler>::InboundProtocol,
        >,
    ) {
        if let Some(h) = self.handlers.get_mut(&key) {
            if let Some(i) = info.take(&key) {
                h.on_connection_event(ConnectionEvent::ListenUpgradeError(ListenUpgradeError {
                    info: i,
                    error,
                }));
            }
        }
    }
}

impl<K, H> ConnectionHandler for MultiHandler<K, H>
where
    K: Clone + Debug + Hash + Eq + Send + 'static,
    H: ConnectionHandler,
    H::InboundProtocol: InboundUpgradeSend,
    H::OutboundProtocol: OutboundUpgradeSend,
{
    type FromBehaviour = (K, <H as ConnectionHandler>::FromBehaviour);
    type ToBehaviour = (K, <H as ConnectionHandler>::ToBehaviour);
    type InboundProtocol = Upgrade<K, <H as ConnectionHandler>::InboundProtocol>;
    type OutboundProtocol = <H as ConnectionHandler>::OutboundProtocol;
    type InboundOpenInfo = Info<K, <H as ConnectionHandler>::InboundOpenInfo>;
    type OutboundOpenInfo = (K, <H as ConnectionHandler>::OutboundOpenInfo);

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        let (upgrade, info, timeout) = self
            .handlers
            .iter()
            .map(|(key, handler)| {
                let proto = handler.listen_protocol();
                let timeout = *proto.timeout();
                let (upgrade, info) = proto.into_upgrade();
                (key.clone(), (upgrade, info, timeout))
            })
            .fold(
                (Upgrade::new(), Info::new(), Duration::from_secs(0)),
                |(mut upg, mut inf, mut timeout), (k, (u, i, t))| {
                    upg.upgrades.push((k.clone(), u));
                    inf.infos.push((k, i));
                    timeout = cmp::max(timeout, t);
                    (upg, inf, timeout)
                },
            );
        SubstreamProtocol::new(upgrade, info).with_timeout(timeout)
    }

    fn on_connection_event(
        &mut self,
        event: ConnectionEvent<
            Self::InboundProtocol,
            Self::OutboundProtocol,
            Self::InboundOpenInfo,
            Self::OutboundOpenInfo,
        >,
    ) {
        match event {
            ConnectionEvent::FullyNegotiatedOutbound(FullyNegotiatedOutbound {
                protocol,
                info: (key, arg),
            }) => {
                if let Some(h) = self.handlers.get_mut(&key) {
                    h.on_connection_event(ConnectionEvent::FullyNegotiatedOutbound(
                        FullyNegotiatedOutbound {
                            protocol,
                            info: arg,
                        },
                    ));
                } else {
                    tracing::error!("FullyNegotiatedOutbound: no handler for key")
                }
            }
            ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
                protocol: (key, arg),
                mut info,
            }) => {
                if let Some(h) = self.handlers.get_mut(&key) {
                    if let Some(i) = info.take(&key) {
                        h.on_connection_event(ConnectionEvent::FullyNegotiatedInbound(
                            FullyNegotiatedInbound {
                                protocol: arg,
                                info: i,
                            },
                        ));
                    }
                } else {
                    tracing::error!("FullyNegotiatedInbound: no handler for key")
                }
            }
            ConnectionEvent::AddressChange(AddressChange { new_address }) => {
                for h in self.handlers.values_mut() {
                    h.on_connection_event(ConnectionEvent::AddressChange(AddressChange {
                        new_address,
                    }));
                }
            }
            ConnectionEvent::DialUpgradeError(DialUpgradeError {
                info: (key, arg),
                error,
            }) => {
                if let Some(h) = self.handlers.get_mut(&key) {
                    h.on_connection_event(ConnectionEvent::DialUpgradeError(DialUpgradeError {
                        info: arg,
                        error,
                    }));
                } else {
                    tracing::error!("DialUpgradeError: no handler for protocol")
                }
            }
            ConnectionEvent::ListenUpgradeError(listen_upgrade_error) => {
                self.on_listen_upgrade_error(listen_upgrade_error)
            }
            ConnectionEvent::LocalProtocolsChange(supported_protocols) => {
                for h in self.handlers.values_mut() {
                    h.on_connection_event(ConnectionEvent::LocalProtocolsChange(
                        supported_protocols.clone(),
                    ));
                }
            }
            ConnectionEvent::RemoteProtocolsChange(supported_protocols) => {
                for h in self.handlers.values_mut() {
                    h.on_connection_event(ConnectionEvent::RemoteProtocolsChange(
                        supported_protocols.clone(),
                    ));
                }
            }
        }
    }

    fn on_behaviour_event(&mut self, (key, event): Self::FromBehaviour) {
        if let Some(h) = self.handlers.get_mut(&key) {
            h.on_behaviour_event(event)
        } else {
            tracing::error!("on_behaviour_event: no handler for key")
        }
    }

    fn connection_keep_alive(&self) -> bool {
        self.handlers
            .values()
            .map(|h| h.connection_keep_alive())
            .max()
            .unwrap_or(false)
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<Self::OutboundProtocol, Self::OutboundOpenInfo, Self::ToBehaviour>,
    > {
        // Calling `gen_range(0, 0)` (see below) would panic, so we have return early to avoid
        // that situation.
        if self.handlers.is_empty() {
            return Poll::Pending;
        }

        // Not always polling handlers in the same order should give anyone the chance to make progress.
        let pos = rand::thread_rng().gen_range(0..self.handlers.len());

        for (k, h) in self.handlers.iter_mut().skip(pos) {
            if let Poll::Ready(e) = h.poll(cx) {
                let e = e
                    .map_outbound_open_info(|i| (k.clone(), i))
                    .map_custom(|p| (k.clone(), p));
                return Poll::Ready(e);
            }
        }

        for (k, h) in self.handlers.iter_mut().take(pos) {
            if let Poll::Ready(e) = h.poll(cx) {
                let e = e
                    .map_outbound_open_info(|i| (k.clone(), i))
                    .map_custom(|p| (k.clone(), p));
                return Poll::Ready(e);
            }
        }

        Poll::Pending
    }

    fn poll_close(&mut self, cx: &mut Context<'_>) -> Poll<Option<Self::ToBehaviour>> {
        for (k, h) in self.handlers.iter_mut() {
            let Some(e) = ready!(h.poll_close(cx)) else {
                continue;
            };
            return Poll::Ready(Some((k.clone(), e)));
        }

        Poll::Ready(None)
    }
}

/// Split [`MultiHandler`] into parts.
impl<K, H> IntoIterator for MultiHandler<K, H> {
    type Item = <Self::IntoIter as Iterator>::Item;
    type IntoIter = std::collections::hash_map::IntoIter<K, H>;

    fn into_iter(self) -> Self::IntoIter {
        self.handlers.into_iter()
    }
}

/// Index and protocol name pair used as `UpgradeInfo::Info`.
#[derive(Debug, Clone)]
pub struct IndexedProtoName<H>(usize, H);

impl<H: AsRef<str>> AsRef<str> for IndexedProtoName<H> {
    fn as_ref(&self) -> &str {
        self.1.as_ref()
    }
}

/// The aggregated `InboundOpenInfo`s of supported inbound substream protocols.
#[derive(Clone)]
pub struct Info<K, I> {
    infos: Vec<(K, I)>,
}

impl<K: Eq, I> Info<K, I> {
    fn new() -> Self {
        Info { infos: Vec::new() }
    }

    pub fn take(&mut self, k: &K) -> Option<I> {
        if let Some(p) = self.infos.iter().position(|(key, _)| key == k) {
            return Some(self.infos.remove(p).1);
        }
        None
    }
}

/// Inbound and outbound upgrade for all [`ConnectionHandler`]s.
#[derive(Clone)]
pub struct Upgrade<K, H> {
    upgrades: Vec<(K, H)>,
}

impl<K, H> Upgrade<K, H> {
    fn new() -> Self {
        Upgrade {
            upgrades: Vec::new(),
        }
    }
}

impl<K, H> fmt::Debug for Upgrade<K, H>
where
    K: fmt::Debug + Eq + Hash,
    H: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Upgrade")
            .field("upgrades", &self.upgrades)
            .finish()
    }
}

impl<K, H> UpgradeInfoSend for Upgrade<K, H>
where
    H: UpgradeInfoSend,
    K: Send + 'static,
{
    type Info = IndexedProtoName<H::Info>;
    type InfoIter = std::vec::IntoIter<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        self.upgrades
            .iter()
            .enumerate()
            .flat_map(|(i, (_, h))| iter::repeat(i).zip(h.protocol_info()))
            .map(|(i, h)| IndexedProtoName(i, h))
            .collect::<Vec<_>>()
            .into_iter()
    }
}

impl<K, H> InboundUpgradeSend for Upgrade<K, H>
where
    H: InboundUpgradeSend,
    K: Send + 'static,
{
    type Output = (K, <H as InboundUpgradeSend>::Output);
    type Error = (K, <H as InboundUpgradeSend>::Error);
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    fn upgrade_inbound(mut self, resource: Stream, info: Self::Info) -> Self::Future {
        let IndexedProtoName(index, info) = info;
        let (key, upgrade) = self.upgrades.remove(index);
        upgrade
            .upgrade_inbound(resource, info)
            .map(move |out| match out {
                Ok(o) => Ok((key, o)),
                Err(e) => Err((key, e)),
            })
            .boxed()
    }
}

impl<K, H> OutboundUpgradeSend for Upgrade<K, H>
where
    H: OutboundUpgradeSend,
    K: Send + 'static,
{
    type Output = (K, <H as OutboundUpgradeSend>::Output);
    type Error = (K, <H as OutboundUpgradeSend>::Error);
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    fn upgrade_outbound(mut self, resource: Stream, info: Self::Info) -> Self::Future {
        let IndexedProtoName(index, info) = info;
        let (key, upgrade) = self.upgrades.remove(index);
        upgrade
            .upgrade_outbound(resource, info)
            .map(move |out| match out {
                Ok(o) => Ok((key, o)),
                Err(e) => Err((key, e)),
            })
            .boxed()
    }
}

/// Check that no two protocol names are equal.
fn uniq_proto_names<I, T>(iter: I) -> Result<(), DuplicateProtonameError>
where
    I: Iterator<Item = T>,
    T: UpgradeInfoSend,
{
    let mut set = HashSet::new();
    for infos in iter {
        for i in infos.protocol_info() {
            let v = Vec::from(i.as_ref());
            if set.contains(&v) {
                return Err(DuplicateProtonameError(v));
            } else {
                set.insert(v);
            }
        }
    }
    Ok(())
}

/// It is an error if two handlers share the same protocol name.
#[derive(Debug, Clone)]
pub struct DuplicateProtonameError(Vec<u8>);

impl DuplicateProtonameError {
    /// The protocol name bytes that occurred in more than one handler.
    pub fn protocol_name(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Display for DuplicateProtonameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Ok(s) = std::str::from_utf8(&self.0) {
            write!(f, "duplicate protocol name: {s}")
        } else {
            write!(f, "duplicate protocol name: {:?}", self.0)
        }
    }
}

impl error::Error for DuplicateProtonameError {}
