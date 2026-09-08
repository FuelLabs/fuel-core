use futures::{
    future::poll_fn,
    io::{
        AsyncRead,
        AsyncWrite,
        Cursor,
    },
};
use libp2p::core::{
    muxing::StreamMuxer,
    upgrade::InboundConnectionUpgrade,
};
use std::{
    io,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

// Keep inbound bytes separate from the replies written by the muxer.
struct Wire(Cursor<Vec<u8>>);

impl AsyncRead for Wire {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.0).poll_read(cx, buf)
    }
}

impl AsyncWrite for Wire {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

fn header(tag: u8, flags: u16, len: u32) -> Vec<u8> {
    let mut frame = vec![0, tag];
    frame.extend(flags.to_be_bytes());
    frame.extend(1u32.to_be_bytes());
    frame.extend(len.to_be_bytes());
    frame
}

async fn rejects_malformed_frames(bytes: Vec<u8>, opens_stream: bool) {
    // Exercise the same default adapter used by FuelP2PService, not a separate
    // direct yamux dependency that could diverge from the transport's version.
    let mut mux = libp2p::yamux::Config::default()
        .upgrade_inbound(Wire(Cursor::new(bytes)), "/yamux/1.0.0")
        .await
        .unwrap();
    let stream = if opens_stream {
        Some(
            poll_fn(|cx| Pin::new(&mut mux).poll_inbound(cx))
                .await
                .unwrap(),
        )
    } else {
        None
    };
    let result = poll_fn(|cx| Pin::new(&mut mux).poll_inbound(cx)).await;
    assert!(result.is_err(), "malformed input must close the connection");
    // The oversized SYN bug also panics during stream/connection cleanup.
    drop(stream);
    drop(mux);
}

#[tokio::test]
async fn oversized_data_syn_closes_connection_without_panicking() {
    // GHSA-vxx9-2994-q338: DEFAULT_CREDIT + 1 bytes on a new inbound stream.
    let mut bytes = header(0, 1, 262145);
    bytes.resize(262157, 0);
    rejects_malformed_frames(bytes, false).await;
}

#[tokio::test]
async fn overflowing_window_update_closes_connection_without_panicking() {
    // GHSA-4w32-2493-32g7: a valid stream followed by overflowing send credit.
    let mut bytes = header(0, 1, 0);
    bytes.extend(header(1, 0, 0xffff0000));
    rejects_malformed_frames(bytes, true).await;
}
