use super::HEARTBEAT_PROTOCOL;
use fuel_core_types::blockchain::primitives::BlockHeight;
use futures::{
    future::BoxFuture,
    AsyncRead,
    AsyncReadExt,
    AsyncWrite,
    AsyncWriteExt,
    FutureExt,
};
use libp2p_core::upgrade::ReadyUpgrade;
use libp2p_swarm::{
    handler::{
        ConnectionEvent,
        FullyNegotiatedInbound,
        FullyNegotiatedOutbound,
    },
    ConnectionHandler,
    ConnectionHandlerEvent,
    KeepAlive,
    NegotiatedSubstream,
    SubstreamProtocol,
};
use std::{
    fmt::Display,
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
pub struct HeartbeatConfig {
    /// Sending of `BlockHeight` should not take longer than this
    send_timeout: Duration,
    /// Idle time before sending next `BlockHeight`
    idle_timeout: Duration,
    /// Max failures allowed.
    /// If reached `HeartbeatHandler` will request closing of the connection.
    max_failures: NonZeroU32,
}

impl HeartbeatConfig {
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

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self::new(
            Duration::from_secs(2),
            Duration::from_secs(1),
            NonZeroU32::new(5).expect("5 != 0"),
        )
    }
}

#[derive(Debug)]
pub enum HeartbeatFailure {
    Timeout,
}
impl Display for HeartbeatFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HeartbeatFailure::Timeout => f.write_str("Heartbeat timeout"),
        }
    }
}
impl std::error::Error for HeartbeatFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            HeartbeatFailure::Timeout => None,
        }
    }
}

type InboundData =
    BoxFuture<'static, Result<(NegotiatedSubstream, BlockHeight), std::io::Error>>;
type OutboundData = BoxFuture<'static, Result<NegotiatedSubstream, std::io::Error>>;

pub struct HeartbeatHandler {
    config: HeartbeatConfig,
    inbound: Option<InboundData>,
    outbound: Option<OutboundState>,
    timer: Pin<Box<Sleep>>,
    failure_count: u32,
}

impl HeartbeatHandler {
    pub fn new(config: HeartbeatConfig) -> Self {
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
    type InEvent = HeartbeatInEvent;
    type OutEvent = HeartbeatOutEvent;
    type Error = HeartbeatFailure;

    type InboundProtocol = ReadyUpgrade<&'static [u8]>;
    type OutboundProtocol = ReadyUpgrade<&'static [u8]>;
    type OutboundOpenInfo = ();
    type InboundOpenInfo = ();

    fn listen_protocol(&self) -> SubstreamProtocol<ReadyUpgrade<&'static [u8]>, ()> {
        SubstreamProtocol::new(ReadyUpgrade::new(HEARTBEAT_PROTOCOL), ())
    }

    fn connection_keep_alive(&self) -> KeepAlive {
        // Heartbeat protocol wants to keep the connection alive
        KeepAlive::Yes
    }

    fn on_behaviour_event(&mut self, event: Self::InEvent) {
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

    fn poll(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<
        ConnectionHandlerEvent<
            Self::OutboundProtocol,
            Self::OutboundOpenInfo,
            Self::OutEvent,
            Self::Error,
        >,
    > {
        if let Some(inbound_block_height) = self.inbound.as_mut() {
            match inbound_block_height.poll_unpin(cx) {
                Poll::Ready(Err(_)) => {
                    debug!(target: "fuel-libp2p", "Incoming heartbeat errored");
                    self.inbound = None;
                }
                Poll::Ready(Ok((stream, block_height))) => {
                    // start waiting for the next `BlockHeight`
                    self.inbound = Some(receive_block_height(stream).boxed());

                    // report newly received `BlockHeight` to the Behaviour
                    return Poll::Ready(ConnectionHandlerEvent::Custom(
                        HeartbeatOutEvent::BlockHeight(block_height),
                    ))
                }
                _ => {}
            }
        }

        loop {
            if self.failure_count >= self.config.max_failures.into() {
                // Request from `Swarm` to close the faulty connection
                return Poll::Ready(ConnectionHandlerEvent::Close(
                    HeartbeatFailure::Timeout,
                ))
            }

            match self.outbound.take() {
                Some(OutboundState::RequestingBlockHeight { requested, stream }) => {
                    self.outbound = Some(OutboundState::RequestingBlockHeight {
                        stream,
                        requested: true,
                    });

                    if !requested {
                        return Poll::Ready(ConnectionHandlerEvent::Custom(
                            HeartbeatOutEvent::RequestBlockHeight,
                        ))
                    }

                    break
                }
                Some(OutboundState::SendingBlockHeight(mut outbound_block_height)) => {
                    match outbound_block_height.poll_unpin(cx) {
                        Poll::Pending => {
                            if self.timer.poll_unpin(cx).is_ready() {
                                // Time for successfull send expired!
                                self.failure_count += 1;
                                debug!(target: "fuel-libp2p", "Sending Heartbeat timed out, this is {} time it failed with this connection", self.failure_count);
                            } else {
                                self.outbound = Some(OutboundState::SendingBlockHeight(
                                    outbound_block_height,
                                ));
                                break
                            }
                        }
                        Poll::Ready(Ok(stream)) => {
                            // reset failure count
                            self.failure_count = 0;
                            // start new idle timeout until next request & send
                            self.timer = Box::pin(sleep(self.config.idle_timeout));
                            self.outbound = Some(OutboundState::Idle(stream));
                        }
                        Poll::Ready(Err(_)) => {
                            self.failure_count += 1;
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
                self.failure_count += 1;
            }
            _ => {}
        }
    }
}

/// Represents state of the Oubound stream
enum OutboundState {
    NegotiatingStream,
    Idle(NegotiatedSubstream),
    RequestingBlockHeight {
        stream: NegotiatedSubstream,
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
