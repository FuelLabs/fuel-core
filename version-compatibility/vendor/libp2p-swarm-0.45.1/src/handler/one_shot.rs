// Copyright 2019 Parity Technologies (UK) Ltd.
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
    ConnectionEvent, ConnectionHandler, ConnectionHandlerEvent, DialUpgradeError,
    FullyNegotiatedInbound, FullyNegotiatedOutbound, SubstreamProtocol,
};
use crate::upgrade::{InboundUpgradeSend, OutboundUpgradeSend};
use crate::StreamUpgradeError;
use smallvec::SmallVec;
use std::{error, fmt::Debug, task::Context, task::Poll, time::Duration};

/// A [`ConnectionHandler`] that opens a new substream for each request.
// TODO: Debug
pub struct OneShotHandler<TInbound, TOutbound, TEvent>
where
    TOutbound: OutboundUpgradeSend,
{
    /// The upgrade for inbound substreams.
    listen_protocol: SubstreamProtocol<TInbound, ()>,
    /// Queue of events to produce in `poll()`.
    events_out: SmallVec<[Result<TEvent, StreamUpgradeError<TOutbound::Error>>; 4]>,
    /// Queue of outbound substreams to open.
    dial_queue: SmallVec<[TOutbound; 4]>,
    /// Current number of concurrent outbound substreams being opened.
    dial_negotiated: u32,
    /// The configuration container for the handler
    config: OneShotHandlerConfig,
}

impl<TInbound, TOutbound, TEvent> OneShotHandler<TInbound, TOutbound, TEvent>
where
    TOutbound: OutboundUpgradeSend,
{
    /// Creates a `OneShotHandler`.
    pub fn new(
        listen_protocol: SubstreamProtocol<TInbound, ()>,
        config: OneShotHandlerConfig,
    ) -> Self {
        OneShotHandler {
            listen_protocol,
            events_out: SmallVec::new(),
            dial_queue: SmallVec::new(),
            dial_negotiated: 0,
            config,
        }
    }

    /// Returns the number of pending requests.
    pub fn pending_requests(&self) -> u32 {
        self.dial_negotiated + self.dial_queue.len() as u32
    }

    /// Returns a reference to the listen protocol configuration.
    ///
    /// > **Note**: If you modify the protocol, modifications will only applies to future inbound
    /// >           substreams, not the ones already being negotiated.
    pub fn listen_protocol_ref(&self) -> &SubstreamProtocol<TInbound, ()> {
        &self.listen_protocol
    }

    /// Returns a mutable reference to the listen protocol configuration.
    ///
    /// > **Note**: If you modify the protocol, modifications will only applies to future inbound
    /// >           substreams, not the ones already being negotiated.
    pub fn listen_protocol_mut(&mut self) -> &mut SubstreamProtocol<TInbound, ()> {
        &mut self.listen_protocol
    }

    /// Opens an outbound substream with `upgrade`.
    pub fn send_request(&mut self, upgrade: TOutbound) {
        self.dial_queue.push(upgrade);
    }
}

impl<TInbound, TOutbound, TEvent> Default for OneShotHandler<TInbound, TOutbound, TEvent>
where
    TOutbound: OutboundUpgradeSend,
    TInbound: InboundUpgradeSend + Default,
{
    fn default() -> Self {
        OneShotHandler::new(
            SubstreamProtocol::new(Default::default(), ()),
            OneShotHandlerConfig::default(),
        )
    }
}

impl<TInbound, TOutbound, TEvent> ConnectionHandler for OneShotHandler<TInbound, TOutbound, TEvent>
where
    TInbound: InboundUpgradeSend + Send + 'static,
    TOutbound: Debug + OutboundUpgradeSend,
    TInbound::Output: Into<TEvent>,
    TOutbound::Output: Into<TEvent>,
    TOutbound::Error: error::Error + Send + 'static,
    SubstreamProtocol<TInbound, ()>: Clone,
    TEvent: Debug + Send + 'static,
{
    type FromBehaviour = TOutbound;
    type ToBehaviour = Result<TEvent, StreamUpgradeError<TOutbound::Error>>;
    type InboundProtocol = TInbound;
    type OutboundProtocol = TOutbound;
    type OutboundOpenInfo = ();
    type InboundOpenInfo = ();

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        self.listen_protocol.clone()
    }

    fn on_behaviour_event(&mut self, event: Self::FromBehaviour) {
        self.send_request(event);
    }

    fn poll(
        &mut self,
        _: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<Self::OutboundProtocol, Self::OutboundOpenInfo, Self::ToBehaviour>,
    > {
        if !self.events_out.is_empty() {
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(
                self.events_out.remove(0),
            ));
        } else {
            self.events_out.shrink_to_fit();
        }

        if !self.dial_queue.is_empty() {
            if self.dial_negotiated < self.config.max_dial_negotiated {
                self.dial_negotiated += 1;
                let upgrade = self.dial_queue.remove(0);
                return Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest {
                    protocol: SubstreamProtocol::new(upgrade, ())
                        .with_timeout(self.config.outbound_substream_timeout),
                });
            }
        } else {
            self.dial_queue.shrink_to_fit();
        }

        Poll::Pending
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
            ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
                protocol: out,
                ..
            }) => {
                self.events_out.push(Ok(out.into()));
            }
            ConnectionEvent::FullyNegotiatedOutbound(FullyNegotiatedOutbound {
                protocol: out,
                ..
            }) => {
                self.dial_negotiated -= 1;
                self.events_out.push(Ok(out.into()));
            }
            ConnectionEvent::DialUpgradeError(DialUpgradeError { error, .. }) => {
                self.events_out.push(Err(error));
            }
            ConnectionEvent::AddressChange(_)
            | ConnectionEvent::ListenUpgradeError(_)
            | ConnectionEvent::LocalProtocolsChange(_)
            | ConnectionEvent::RemoteProtocolsChange(_) => {}
        }
    }
}

/// Configuration parameters for the `OneShotHandler`
#[derive(Debug)]
pub struct OneShotHandlerConfig {
    /// Timeout for outbound substream upgrades.
    pub outbound_substream_timeout: Duration,
    /// Maximum number of concurrent outbound substreams being opened.
    pub max_dial_negotiated: u32,
}

impl Default for OneShotHandlerConfig {
    fn default() -> Self {
        OneShotHandlerConfig {
            outbound_substream_timeout: Duration::from_secs(10),
            max_dial_negotiated: 8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures::executor::block_on;
    use futures::future::poll_fn;
    use libp2p_core::upgrade::DeniedUpgrade;
    use void::Void;

    #[test]
    fn do_not_keep_idle_connection_alive() {
        let mut handler: OneShotHandler<_, DeniedUpgrade, Void> = OneShotHandler::new(
            SubstreamProtocol::new(DeniedUpgrade {}, ()),
            Default::default(),
        );

        block_on(poll_fn(|cx| loop {
            if handler.poll(cx).is_pending() {
                return Poll::Ready(());
            }
        }));

        assert!(!handler.connection_keep_alive());
    }
}
