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

use crate::{transport::TransportError, Multiaddr};
use futures::{
    future::{BoxFuture, Future},
    ready,
    stream::{FuturesUnordered, StreamExt},
};
use libp2p_core::muxing::StreamMuxerBox;
use libp2p_identity::PeerId;
use std::{
    num::NonZeroU8,
    pin::Pin,
    task::{Context, Poll},
};

type Dial = BoxFuture<
    'static,
    (
        Multiaddr,
        Result<(PeerId, StreamMuxerBox), TransportError<std::io::Error>>,
    ),
>;

pub(crate) struct ConcurrentDial {
    dials: FuturesUnordered<Dial>,
    pending_dials: Box<dyn Iterator<Item = Dial> + Send>,
    errors: Vec<(Multiaddr, TransportError<std::io::Error>)>,
}

impl Unpin for ConcurrentDial {}

impl ConcurrentDial {
    pub(crate) fn new(pending_dials: Vec<Dial>, concurrency_factor: NonZeroU8) -> Self {
        let mut pending_dials = pending_dials.into_iter();

        let dials = FuturesUnordered::new();
        for dial in pending_dials.by_ref() {
            dials.push(dial);
            if dials.len() == concurrency_factor.get() as usize {
                break;
            }
        }

        Self {
            dials,
            errors: Default::default(),
            pending_dials: Box::new(pending_dials),
        }
    }
}

impl Future for ConcurrentDial {
    type Output = Result<
        // Either one dial succeeded, returning the negotiated [`PeerId`], the address, the
        // muxer and the addresses and errors of the dials that failed before.
        (
            Multiaddr,
            (PeerId, StreamMuxerBox),
            Vec<(Multiaddr, TransportError<std::io::Error>)>,
        ),
        // Or all dials failed, thus returning the address and error for each dial.
        Vec<(Multiaddr, TransportError<std::io::Error>)>,
    >;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        loop {
            match ready!(self.dials.poll_next_unpin(cx)) {
                Some((addr, Ok(output))) => {
                    let errors = std::mem::take(&mut self.errors);
                    return Poll::Ready(Ok((addr, output, errors)));
                }
                Some((addr, Err(e))) => {
                    self.errors.push((addr, e));
                    if let Some(dial) = self.pending_dials.next() {
                        self.dials.push(dial)
                    }
                }
                None => {
                    return Poll::Ready(Err(std::mem::take(&mut self.errors)));
                }
            }
        }
    }
}
