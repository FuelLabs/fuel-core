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

use crate::handler::{
    ConnectionEvent, ConnectionHandler, ConnectionHandlerEvent, SubstreamProtocol,
};
use std::{fmt::Debug, marker::PhantomData, task::Context, task::Poll};

/// Wrapper around a protocol handler that turns the input event into something else.
#[derive(Debug)]
pub struct MapInEvent<TConnectionHandler, TNewIn, TMap> {
    inner: TConnectionHandler,
    map: TMap,
    marker: PhantomData<TNewIn>,
}

impl<TConnectionHandler, TMap, TNewIn> MapInEvent<TConnectionHandler, TNewIn, TMap> {
    /// Creates a `MapInEvent`.
    pub(crate) fn new(inner: TConnectionHandler, map: TMap) -> Self {
        MapInEvent {
            inner,
            map,
            marker: PhantomData,
        }
    }
}

impl<TConnectionHandler, TMap, TNewIn> ConnectionHandler
    for MapInEvent<TConnectionHandler, TNewIn, TMap>
where
    TConnectionHandler: ConnectionHandler,
    TMap: Fn(TNewIn) -> Option<TConnectionHandler::FromBehaviour>,
    TNewIn: Debug + Send + 'static,
    TMap: Send + 'static,
{
    type FromBehaviour = TNewIn;
    type ToBehaviour = TConnectionHandler::ToBehaviour;
    type InboundProtocol = TConnectionHandler::InboundProtocol;
    type OutboundProtocol = TConnectionHandler::OutboundProtocol;
    type InboundOpenInfo = TConnectionHandler::InboundOpenInfo;
    type OutboundOpenInfo = TConnectionHandler::OutboundOpenInfo;

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        self.inner.listen_protocol()
    }

    fn on_behaviour_event(&mut self, event: TNewIn) {
        if let Some(event) = (self.map)(event) {
            self.inner.on_behaviour_event(event);
        }
    }

    fn connection_keep_alive(&self) -> bool {
        self.inner.connection_keep_alive()
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<Self::OutboundProtocol, Self::OutboundOpenInfo, Self::ToBehaviour>,
    > {
        self.inner.poll(cx)
    }

    fn poll_close(&mut self, cx: &mut Context<'_>) -> Poll<Option<Self::ToBehaviour>> {
        self.inner.poll_close(cx)
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
        self.inner.on_connection_event(event);
    }
}
