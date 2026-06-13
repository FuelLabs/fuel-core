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

use crate::handler::{
    ConnectionEvent, ConnectionHandler, ConnectionHandlerEvent, FullyNegotiatedInbound,
    InboundUpgradeSend, ListenUpgradeError, SubstreamProtocol,
};
use crate::upgrade::SendWrapper;
use either::Either;
use futures::future;
use std::task::{Context, Poll};

impl<LIP, RIP, LIOI, RIOI>
    FullyNegotiatedInbound<Either<SendWrapper<LIP>, SendWrapper<RIP>>, Either<LIOI, RIOI>>
where
    RIP: InboundUpgradeSend,
    LIP: InboundUpgradeSend,
{
    pub(crate) fn transpose(
        self,
    ) -> Either<FullyNegotiatedInbound<LIP, LIOI>, FullyNegotiatedInbound<RIP, RIOI>> {
        match self {
            FullyNegotiatedInbound {
                protocol: future::Either::Left(protocol),
                info: Either::Left(info),
            } => Either::Left(FullyNegotiatedInbound { protocol, info }),
            FullyNegotiatedInbound {
                protocol: future::Either::Right(protocol),
                info: Either::Right(info),
            } => Either::Right(FullyNegotiatedInbound { protocol, info }),
            _ => unreachable!(),
        }
    }
}

impl<LIP, RIP, LIOI, RIOI>
    ListenUpgradeError<Either<LIOI, RIOI>, Either<SendWrapper<LIP>, SendWrapper<RIP>>>
where
    RIP: InboundUpgradeSend,
    LIP: InboundUpgradeSend,
{
    fn transpose(self) -> Either<ListenUpgradeError<LIOI, LIP>, ListenUpgradeError<RIOI, RIP>> {
        match self {
            ListenUpgradeError {
                error: Either::Left(error),
                info: Either::Left(info),
            } => Either::Left(ListenUpgradeError { error, info }),
            ListenUpgradeError {
                error: Either::Right(error),
                info: Either::Right(info),
            } => Either::Right(ListenUpgradeError { error, info }),
            _ => unreachable!(),
        }
    }
}

/// Implementation of a [`ConnectionHandler`] that represents either of two [`ConnectionHandler`]
/// implementations.
impl<L, R> ConnectionHandler for Either<L, R>
where
    L: ConnectionHandler,
    R: ConnectionHandler,
{
    type FromBehaviour = Either<L::FromBehaviour, R::FromBehaviour>;
    type ToBehaviour = Either<L::ToBehaviour, R::ToBehaviour>;
    type InboundProtocol = Either<SendWrapper<L::InboundProtocol>, SendWrapper<R::InboundProtocol>>;
    type OutboundProtocol =
        Either<SendWrapper<L::OutboundProtocol>, SendWrapper<R::OutboundProtocol>>;
    type InboundOpenInfo = Either<L::InboundOpenInfo, R::InboundOpenInfo>;
    type OutboundOpenInfo = Either<L::OutboundOpenInfo, R::OutboundOpenInfo>;

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        match self {
            Either::Left(a) => a
                .listen_protocol()
                .map_upgrade(|u| Either::Left(SendWrapper(u)))
                .map_info(Either::Left),
            Either::Right(b) => b
                .listen_protocol()
                .map_upgrade(|u| Either::Right(SendWrapper(u)))
                .map_info(Either::Right),
        }
    }

    fn on_behaviour_event(&mut self, event: Self::FromBehaviour) {
        match (self, event) {
            (Either::Left(handler), Either::Left(event)) => handler.on_behaviour_event(event),
            (Either::Right(handler), Either::Right(event)) => handler.on_behaviour_event(event),
            _ => unreachable!(),
        }
    }

    fn connection_keep_alive(&self) -> bool {
        match self {
            Either::Left(handler) => handler.connection_keep_alive(),
            Either::Right(handler) => handler.connection_keep_alive(),
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<Self::OutboundProtocol, Self::OutboundOpenInfo, Self::ToBehaviour>,
    > {
        let event = match self {
            Either::Left(handler) => futures::ready!(handler.poll(cx))
                .map_custom(Either::Left)
                .map_protocol(|p| Either::Left(SendWrapper(p)))
                .map_outbound_open_info(Either::Left),
            Either::Right(handler) => futures::ready!(handler.poll(cx))
                .map_custom(Either::Right)
                .map_protocol(|p| Either::Right(SendWrapper(p)))
                .map_outbound_open_info(Either::Right),
        };

        Poll::Ready(event)
    }

    fn poll_close(&mut self, cx: &mut Context<'_>) -> Poll<Option<Self::ToBehaviour>> {
        let event = match self {
            Either::Left(handler) => futures::ready!(handler.poll_close(cx)).map(Either::Left),
            Either::Right(handler) => futures::ready!(handler.poll_close(cx)).map(Either::Right),
        };

        Poll::Ready(event)
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
            ConnectionEvent::FullyNegotiatedInbound(fully_negotiated_inbound) => {
                match (fully_negotiated_inbound.transpose(), self) {
                    (Either::Left(fully_negotiated_inbound), Either::Left(handler)) => handler
                        .on_connection_event(ConnectionEvent::FullyNegotiatedInbound(
                            fully_negotiated_inbound,
                        )),
                    (Either::Right(fully_negotiated_inbound), Either::Right(handler)) => handler
                        .on_connection_event(ConnectionEvent::FullyNegotiatedInbound(
                            fully_negotiated_inbound,
                        )),
                    _ => unreachable!(),
                }
            }
            ConnectionEvent::FullyNegotiatedOutbound(fully_negotiated_outbound) => {
                match (fully_negotiated_outbound.transpose(), self) {
                    (Either::Left(fully_negotiated_outbound), Either::Left(handler)) => handler
                        .on_connection_event(ConnectionEvent::FullyNegotiatedOutbound(
                            fully_negotiated_outbound,
                        )),
                    (Either::Right(fully_negotiated_outbound), Either::Right(handler)) => handler
                        .on_connection_event(ConnectionEvent::FullyNegotiatedOutbound(
                            fully_negotiated_outbound,
                        )),
                    _ => unreachable!(),
                }
            }
            ConnectionEvent::DialUpgradeError(dial_upgrade_error) => {
                match (dial_upgrade_error.transpose(), self) {
                    (Either::Left(dial_upgrade_error), Either::Left(handler)) => handler
                        .on_connection_event(ConnectionEvent::DialUpgradeError(dial_upgrade_error)),
                    (Either::Right(dial_upgrade_error), Either::Right(handler)) => handler
                        .on_connection_event(ConnectionEvent::DialUpgradeError(dial_upgrade_error)),
                    _ => unreachable!(),
                }
            }
            ConnectionEvent::ListenUpgradeError(listen_upgrade_error) => {
                match (listen_upgrade_error.transpose(), self) {
                    (Either::Left(listen_upgrade_error), Either::Left(handler)) => handler
                        .on_connection_event(ConnectionEvent::ListenUpgradeError(
                            listen_upgrade_error,
                        )),
                    (Either::Right(listen_upgrade_error), Either::Right(handler)) => handler
                        .on_connection_event(ConnectionEvent::ListenUpgradeError(
                            listen_upgrade_error,
                        )),
                    _ => unreachable!(),
                }
            }
            ConnectionEvent::AddressChange(address_change) => match self {
                Either::Left(handler) => {
                    handler.on_connection_event(ConnectionEvent::AddressChange(address_change))
                }
                Either::Right(handler) => {
                    handler.on_connection_event(ConnectionEvent::AddressChange(address_change))
                }
            },
            ConnectionEvent::LocalProtocolsChange(supported_protocols) => match self {
                Either::Left(handler) => handler.on_connection_event(
                    ConnectionEvent::LocalProtocolsChange(supported_protocols),
                ),
                Either::Right(handler) => handler.on_connection_event(
                    ConnectionEvent::LocalProtocolsChange(supported_protocols),
                ),
            },
            ConnectionEvent::RemoteProtocolsChange(supported_protocols) => match self {
                Either::Left(handler) => handler.on_connection_event(
                    ConnectionEvent::RemoteProtocolsChange(supported_protocols),
                ),
                Either::Right(handler) => handler.on_connection_event(
                    ConnectionEvent::RemoteProtocolsChange(supported_protocols),
                ),
            },
        }
    }
}
