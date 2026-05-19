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

use crate::behaviour::{self, NetworkBehaviour, ToSwarm};
use crate::connection::ConnectionId;
use crate::{ConnectionDenied, THandler, THandlerInEvent, THandlerOutEvent};
use either::Either;
use libp2p_core::transport::PortUse;
use libp2p_core::{Endpoint, Multiaddr};
use libp2p_identity::PeerId;
use std::{task::Context, task::Poll};

/// Implementation of [`NetworkBehaviour`] that can be either of two implementations.
impl<L, R> NetworkBehaviour for Either<L, R>
where
    L: NetworkBehaviour,
    R: NetworkBehaviour,
{
    type ConnectionHandler = Either<THandler<L>, THandler<R>>;
    type ToSwarm = Either<L::ToSwarm, R::ToSwarm>;

    fn handle_pending_inbound_connection(
        &mut self,
        id: ConnectionId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<(), ConnectionDenied> {
        match self {
            Either::Left(a) => a.handle_pending_inbound_connection(id, local_addr, remote_addr),
            Either::Right(b) => b.handle_pending_inbound_connection(id, local_addr, remote_addr),
        }
    }

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        let handler = match self {
            Either::Left(inner) => Either::Left(inner.handle_established_inbound_connection(
                connection_id,
                peer,
                local_addr,
                remote_addr,
            )?),
            Either::Right(inner) => Either::Right(inner.handle_established_inbound_connection(
                connection_id,
                peer,
                local_addr,
                remote_addr,
            )?),
        };

        Ok(handler)
    }

    fn handle_pending_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        maybe_peer: Option<PeerId>,
        addresses: &[Multiaddr],
        effective_role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        let addresses = match self {
            Either::Left(inner) => inner.handle_pending_outbound_connection(
                connection_id,
                maybe_peer,
                addresses,
                effective_role,
            )?,
            Either::Right(inner) => inner.handle_pending_outbound_connection(
                connection_id,
                maybe_peer,
                addresses,
                effective_role,
            )?,
        };

        Ok(addresses)
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        addr: &Multiaddr,
        role_override: Endpoint,
        port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        let handler = match self {
            Either::Left(inner) => Either::Left(inner.handle_established_outbound_connection(
                connection_id,
                peer,
                addr,
                role_override,
                port_use,
            )?),
            Either::Right(inner) => Either::Right(inner.handle_established_outbound_connection(
                connection_id,
                peer,
                addr,
                role_override,
                port_use,
            )?),
        };

        Ok(handler)
    }

    fn on_swarm_event(&mut self, event: behaviour::FromSwarm) {
        match self {
            Either::Left(b) => b.on_swarm_event(event),
            Either::Right(b) => b.on_swarm_event(event),
        }
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        match (self, event) {
            (Either::Left(left), Either::Left(event)) => {
                left.on_connection_handler_event(peer_id, connection_id, event);
            }
            (Either::Right(right), Either::Right(event)) => {
                right.on_connection_handler_event(peer_id, connection_id, event);
            }
            _ => unreachable!(),
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        let event = match self {
            Either::Left(behaviour) => futures::ready!(behaviour.poll(cx))
                .map_out(Either::Left)
                .map_in(Either::Left),
            Either::Right(behaviour) => futures::ready!(behaviour.poll(cx))
                .map_out(Either::Right)
                .map_in(Either::Right),
        };

        Poll::Ready(event)
    }
}
