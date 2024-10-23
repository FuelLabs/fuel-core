use super::HEARTBEAT_PROTOCOL;
use fuel_core_types::fuel_types::BlockHeight;
use futures::{
    future::BoxFuture,
    AsyncRead,
    AsyncReadExt,
    AsyncWrite,
    AsyncWriteExt,
    FutureExt,
};
use libp2p::{
    core::upgrade::ReadyUpgrade,
    swarm::{
        handler::{
            ConnectionEvent,
            FullyNegotiatedInbound,
            FullyNegotiatedOutbound,
        },
        ConnectionHandler,
        ConnectionHandlerEvent,
        Stream,
        SubstreamProtocol,
    },
};
use std::{
    num::NonZeroU32,
    pin::Pin,
    task::Poll,
    time::Duration,
};
use tokio::time::{
    sleep,
    Sleep,
};
use tracing::debug;

#[derive(Debug, Clone)]
pub enum HeartbeatInEvent {
    LatestBlock(BlockHeight),
}

#[derive(Debug, Clone)]
pub enum HeartbeatOutEvent {
    BlockHeight(BlockHeight),
    RequestBlockHeight,
}

#[derive(Debug, Clone)]
pub struct Config {
    /// Sending of `BlockHeight` should not take longer than this
    pub send_timeout: Duration,
    /// Idle time before sending next `BlockHeight`
    pub idle_timeout: Duration,
    /// Max failures allowed.
    /// If reached `HeartbeatHandler` will request closing of the connection.
    pub max_failures: NonZeroU32,
}

impl Config {
    pub fn new(
        send_timeout: Duration,
        idle_timeout: Duration,
        max_failures: NonZeroU32,
    ) -> Self {
        Self {
            send_timeout,
            idle_timeout,
            max_failures,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new(
            Duration::from_secs(1),
            Duration::from_millis(500),
            NonZeroU32::new(5).expect("5 != 0"),
        )
    }
}

type InboundData = BoxFuture<'static, Result<(Stream, BlockHeight), std::io::Error>>;
type OutboundData = BoxFuture<'static, Result<Stream, std::io::Error>>;

pub struct HeartbeatHandler {
    config: Config,
    inbound: Option<InboundData>,
    outbound: Option<OutboundState>,
    timer: Pin<Box<Sleep>>,
    failure_count: u32,
}

impl HeartbeatHandler {
    pub fn new(config: Config) -> Self {
        dbg!("heartbeat handler");
        Self {
            config,
            inbound: None,
            outbound: None,
            timer: Box::pin(sleep(Duration::new(0, 0))),
            failure_count: 0,
        }
    }
}

impl ConnectionHandler for HeartbeatHandler {
    type FromBehaviour = HeartbeatInEvent;
    type ToBehaviour = HeartbeatOutEvent;
    type InboundProtocol = ReadyUpgrade<&'static str>;
    type OutboundProtocol = ReadyUpgrade<&'static str>;
    type InboundOpenInfo = ();
    type OutboundOpenInfo = ();

    fn listen_protocol(&self) -> SubstreamProtocol<ReadyUpgrade<&'static str>, ()> {
        SubstreamProtocol::new(ReadyUpgrade::new(HEARTBEAT_PROTOCOL), ())
    }

    fn connection_keep_alive(&self) -> bool {
        // Heartbeat protocol wants to keep the connection alive
        true
    }

    fn poll(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<
        ConnectionHandlerEvent<
            Self::OutboundProtocol,
            Self::OutboundOpenInfo,
            Self::ToBehaviour,
        >,
    > {
        if let Some(inbound_stream_and_block_height) = self.inbound.as_mut() {
            match inbound_stream_and_block_height.poll_unpin(cx) {
                Poll::Ready(Err(_)) => {
                    debug!(target: "fuel-libp2p", "Incoming heartbeat errored");
                    self.inbound = None;
                }
                Poll::Ready(Ok((stream, block_height))) => {
                    // start waiting for the next `BlockHeight`
                    self.inbound = Some(receive_block_height(stream).boxed());

                    // report newly received `BlockHeight` to the Behaviour
                    return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(
                        HeartbeatOutEvent::BlockHeight(block_height),
                    ))
                }
                _ => {}
            }
        }

        loop {
            // TODO: Close connection properly: https://github.com/FuelLabs/fuel-core/pull/1379
            // if self.failure_count >= self.config.max_failures.into() {
            //     // Request from `Swarm` to close the faulty connection
            //     return Poll::Ready(ConnectionHandlerEvent::Close(
            //         HeartbeatFailure::Timeout,
            //     ))
            // }

            match self.outbound.take() {
                Some(OutboundState::RequestingBlockHeight { requested, stream }) => {
                    self.outbound = Some(OutboundState::RequestingBlockHeight {
                        stream,
                        requested: true,
                    });

                    if !requested {
                        return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(
                            HeartbeatOutEvent::RequestBlockHeight,
                        ))
                    }

                    break
                }
                Some(OutboundState::SendingBlockHeight(mut outbound_block_height)) => {
                    match outbound_block_height.poll_unpin(cx) {
                        Poll::Pending => {
                            if self.timer.poll_unpin(cx).is_ready() {
                                dbg!("send expired");
                                // Time for successful send expired!
                                self.failure_count = self.failure_count.saturating_add(1);
                                debug!(target: "fuel-libp2p", "Sending Heartbeat timed out, this is {} time it failed with this connection", self.failure_count);
                            } else {
                                self.outbound = Some(OutboundState::SendingBlockHeight(
                                    outbound_block_height,
                                ));
                                break
                            }
                        }
                        Poll::Ready(Ok(stream)) => {
                            dbg!("send success");
                            // reset failure count
                            self.failure_count = 0;
                            // start new idle timeout until next request & send
                            self.timer = Box::pin(sleep(self.config.idle_timeout));
                            self.outbound = Some(OutboundState::Idle(stream));
                        }
                        Poll::Ready(Err(_)) => {
                            dbg!("send error");
                            self.failure_count = self.failure_count.saturating_add(1);
                            debug!(target: "fuel-libp2p", "Sending Heartbeat failed, {}/{} failures for this connection", self.failure_count,  self.config.max_failures);
                        }
                    }
                }
                Some(OutboundState::Idle(stream)) => match self.timer.poll_unpin(cx) {
                    Poll::Pending => {
                        self.outbound = Some(OutboundState::Idle(stream));
                        break
                    }
                    Poll::Ready(()) => {
                        self.outbound = Some(OutboundState::RequestingBlockHeight {
                            stream,
                            requested: false,
                        });
                    }
                },
                Some(OutboundState::NegotiatingStream) => {
                    self.outbound = Some(OutboundState::NegotiatingStream);
                    break
                }
                None => {
                    // Request new stream
                    self.outbound = Some(OutboundState::NegotiatingStream);
                    let protocol =
                        SubstreamProtocol::new(ReadyUpgrade::new(HEARTBEAT_PROTOCOL), ())
                            .with_timeout(self.config.send_timeout);
                    return Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest {
                        protocol,
                    })
                }
            }
        }
        Poll::Pending
    }

    fn on_behaviour_event(&mut self, event: Self::FromBehaviour) {
        let HeartbeatInEvent::LatestBlock(block_height) = event;

        match self.outbound.take() {
            Some(OutboundState::RequestingBlockHeight {
                requested: true,
                stream,
            }) => {
                // start new send timeout
                self.timer = Box::pin(sleep(self.config.send_timeout));
                // send latest `BlockHeight`
                self.outbound = Some(OutboundState::SendingBlockHeight(
                    send_block_height(stream, block_height).boxed(),
                ))
            }
            other_state => self.outbound = other_state,
        }
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
                protocol: stream,
                ..
            }) => {
                self.inbound = Some(receive_block_height(stream).boxed());
            }
            ConnectionEvent::FullyNegotiatedOutbound(FullyNegotiatedOutbound {
                protocol: stream,
                ..
            }) => {
                self.outbound = Some(OutboundState::RequestingBlockHeight {
                    stream,
                    requested: false,
                })
            }
            ConnectionEvent::DialUpgradeError(_) => {
                self.outbound = None;
                self.failure_count = self.failure_count.saturating_add(1);
            }
            _ => {}
        }
    }
}

/// Represents state of the Oubound stream
enum OutboundState {
    NegotiatingStream,
    Idle(Stream),
    RequestingBlockHeight {
        stream: Stream,
        /// `false` if the BlockHeight has not been requested yet.
        /// `true` if the BlockHeight has been requested in the current `Heartbeat` cycle.
        requested: bool,
    },
    SendingBlockHeight(OutboundData),
}

const BLOCK_HEIGHT_SIZE: usize = 4;

/// Takes in a stream
/// Waits to receive next `BlockHeight`
/// Returns the flushed stream and the received `BlockHeight`
async fn receive_block_height<S>(mut stream: S) -> std::io::Result<(S, BlockHeight)>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut payload = [0u8; BLOCK_HEIGHT_SIZE];
    stream.read_exact(&mut payload).await?;
    stream.flush().await?;
    let block_height = u32::from_be_bytes(payload).into();
    Ok((stream, block_height))
}

/// Takes in a stream and latest `BlockHeight`
/// Sends the `BlockHeight` and returns back the stream after flushing it
async fn send_block_height<S>(
    mut stream: S,
    block_height: BlockHeight,
) -> std::io::Result<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    stream.write_all(&block_height.to_bytes()).await?;
    stream.flush().await?;

    Ok(stream)
}
