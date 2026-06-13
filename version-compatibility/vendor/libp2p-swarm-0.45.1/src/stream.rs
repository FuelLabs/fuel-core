use futures::{AsyncRead, AsyncWrite};
use libp2p_core::muxing::SubstreamBox;
use libp2p_core::Negotiated;
use std::{
    io::{IoSlice, IoSliceMut},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

/// Counter for the number of active streams on a connection.
#[derive(Debug, Clone)]
pub(crate) struct ActiveStreamCounter(Arc<()>);

impl ActiveStreamCounter {
    pub(crate) fn default() -> Self {
        Self(Arc::new(()))
    }

    pub(crate) fn has_no_active_streams(&self) -> bool {
        self.num_alive_streams() == 1
    }

    fn num_alive_streams(&self) -> usize {
        Arc::strong_count(&self.0)
    }
}

#[derive(Debug)]
pub struct Stream {
    stream: Negotiated<SubstreamBox>,
    counter: Option<ActiveStreamCounter>,
}

impl Stream {
    pub(crate) fn new(stream: Negotiated<SubstreamBox>, counter: ActiveStreamCounter) -> Self {
        Self {
            stream,
            counter: Some(counter),
        }
    }

    /// Ignore this stream in the [Swarm](crate::Swarm)'s connection-keep-alive algorithm.
    ///
    /// By default, any active stream keeps a connection alive. For most protocols,
    /// this is a good default as it ensures that the protocol is completed before
    /// a connection is shut down.
    /// Some protocols like libp2p's [ping](https://github.com/libp2p/specs/blob/master/ping/ping.md)
    /// for example never complete and are of an auxiliary nature.
    /// These protocols should opt-out of the keep alive algorithm using this method.
    pub fn ignore_for_keep_alive(&mut self) {
        self.counter.take();
    }
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.get_mut().stream).poll_read(cx, buf)
    }

    fn poll_read_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.get_mut().stream).poll_read_vectored(cx, bufs)
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.get_mut().stream).poll_write(cx, buf)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.get_mut().stream).poll_write_vectored(cx, bufs)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().stream).poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().stream).poll_close(cx)
    }
}
