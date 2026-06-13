// Copyright 2021 Protocol Labs.
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
use crate::connection::{Connection, ConnectionId, PendingPoint};
use crate::{
    connection::{
        Connected, ConnectionError, IncomingInfo, PendingConnectionError,
        PendingInboundConnectionError, PendingOutboundConnectionError,
    },
    transport::TransportError,
    ConnectedPoint, ConnectionHandler, Executor, Multiaddr, PeerId,
};
use concurrent_dial::ConcurrentDial;
use fnv::FnvHashMap;
use futures::prelude::*;
use futures::stream::SelectAll;
use futures::{
    channel::{mpsc, oneshot},
    future::{poll_fn, BoxFuture, Either},
    ready,
    stream::FuturesUnordered,
};
use libp2p_core::connection::Endpoint;
use libp2p_core::muxing::{StreamMuxerBox, StreamMuxerExt};
use libp2p_core::transport::PortUse;
use std::task::Waker;
use std::{
    collections::HashMap,
    fmt,
    num::{NonZeroU8, NonZeroUsize},
    pin::Pin,
    task::Context,
    task::Poll,
};
use tracing::Instrument;
use void::Void;
use web_time::{Duration, Instant};

mod concurrent_dial;
mod task;

enum ExecSwitch {
    Executor(Box<dyn Executor + Send>),
    LocalSpawn(FuturesUnordered<Pin<Box<dyn Future<Output = ()> + Send>>>),
}

impl ExecSwitch {
    fn advance_local(&mut self, cx: &mut Context) {
        match self {
            ExecSwitch::Executor(_) => {}
            ExecSwitch::LocalSpawn(local) => {
                while let Poll::Ready(Some(())) = local.poll_next_unpin(cx) {}
            }
        }
    }

    #[track_caller]
    fn spawn(&mut self, task: impl Future<Output = ()> + Send + 'static) {
        let task = task.boxed();

        match self {
            Self::Executor(executor) => executor.exec(task),
            Self::LocalSpawn(local) => local.push(task),
        }
    }
}

/// A connection `Pool` manages a set of connections for each peer.
pub(crate) struct Pool<THandler>
where
    THandler: ConnectionHandler,
{
    local_id: PeerId,

    /// The connection counter(s).
    counters: ConnectionCounters,

    /// The managed connections of each peer that are currently considered established.
    established: FnvHashMap<
        PeerId,
        FnvHashMap<ConnectionId, EstablishedConnection<THandler::FromBehaviour>>,
    >,

    /// The pending connections that are currently being negotiated.
    pending: HashMap<ConnectionId, PendingConnection>,

    /// Size of the task command buffer (per task).
    task_command_buffer_size: usize,

    /// Number of addresses concurrently dialed for a single outbound connection attempt.
    dial_concurrency_factor: NonZeroU8,

    /// The configured override for substream protocol upgrades, if any.
    substream_upgrade_protocol_override: Option<libp2p_core::upgrade::Version>,

    /// The maximum number of inbound streams concurrently negotiating on a connection.
    ///
    /// See [`Connection::max_negotiating_inbound_streams`].
    max_negotiating_inbound_streams: usize,

    /// How many [`task::EstablishedConnectionEvent`]s can be buffered before the connection is back-pressured.
    per_connection_event_buffer_size: usize,

    /// The executor to use for running connection tasks. Can either be a global executor
    /// or a local queue.
    executor: ExecSwitch,

    /// Sender distributed to pending tasks for reporting events back
    /// to the pool.
    pending_connection_events_tx: mpsc::Sender<task::PendingConnectionEvent>,

    /// Receiver for events reported from pending tasks.
    pending_connection_events_rx: mpsc::Receiver<task::PendingConnectionEvent>,

    /// Waker in case we haven't established any connections yet.
    no_established_connections_waker: Option<Waker>,

    /// Receivers for events reported from established connections.
    established_connection_events:
        SelectAll<mpsc::Receiver<task::EstablishedConnectionEvent<THandler::ToBehaviour>>>,

    /// Receivers for [`NewConnection`] objects that are dropped.
    new_connection_dropped_listeners: FuturesUnordered<oneshot::Receiver<StreamMuxerBox>>,

    /// How long a connection should be kept alive once it starts idling.
    idle_connection_timeout: Duration,
}

#[derive(Debug)]
pub(crate) struct EstablishedConnection<TInEvent> {
    endpoint: ConnectedPoint,
    /// Channel endpoint to send commands to the task.
    sender: mpsc::Sender<task::Command<TInEvent>>,
}

impl<TInEvent> EstablishedConnection<TInEvent> {
    /// (Asynchronously) sends an event to the connection handler.
    ///
    /// If the handler is not ready to receive the event, either because
    /// it is busy or the connection is about to close, the given event
    /// is returned with an `Err`.
    ///
    /// If execution of this method is preceded by successful execution of
    /// `poll_ready_notify_handler` without another intervening execution
    /// of `notify_handler`, it only fails if the connection is now about
    /// to close.
    pub(crate) fn notify_handler(&mut self, event: TInEvent) -> Result<(), TInEvent> {
        let cmd = task::Command::NotifyHandler(event);
        self.sender.try_send(cmd).map_err(|e| match e.into_inner() {
            task::Command::NotifyHandler(event) => event,
            _ => unreachable!("Expect failed send to return initial event."),
        })
    }

    /// Checks if `notify_handler` is ready to accept an event.
    ///
    /// Returns `Ok(())` if the handler is ready to receive an event via `notify_handler`.
    ///
    /// Returns `Err(())` if the background task associated with the connection
    /// is terminating and the connection is about to close.
    pub(crate) fn poll_ready_notify_handler(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), ()>> {
        self.sender.poll_ready(cx).map_err(|_| ())
    }

    /// Initiates a graceful close of the connection.
    ///
    /// Has no effect if the connection is already closing.
    pub(crate) fn start_close(&mut self) {
        // Clone the sender so that we are guaranteed to have
        // capacity for the close command (every sender gets a slot).
        match self.sender.clone().try_send(task::Command::Close) {
            Ok(()) => {}
            Err(e) => assert!(e.is_disconnected(), "No capacity for close command."),
        };
    }
}

struct PendingConnection {
    /// [`PeerId`] of the remote peer.
    peer_id: Option<PeerId>,
    endpoint: PendingPoint,
    /// When dropped, notifies the task which then knows to terminate.
    abort_notifier: Option<oneshot::Sender<Void>>,
    /// The moment we became aware of this possible connection, useful for timing metrics.
    accepted_at: Instant,
}

impl PendingConnection {
    fn is_for_same_remote_as(&self, other: PeerId) -> bool {
        self.peer_id.map_or(false, |peer| peer == other)
    }

    /// Aborts the connection attempt, closing the connection.
    fn abort(&mut self) {
        if let Some(notifier) = self.abort_notifier.take() {
            drop(notifier);
        }
    }
}

impl<THandler: ConnectionHandler> fmt::Debug for Pool<THandler> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        f.debug_struct("Pool")
            .field("counters", &self.counters)
            .finish()
    }
}

/// Event that can happen on the `Pool`.
#[derive(Debug)]
pub(crate) enum PoolEvent<ToBehaviour> {
    /// A new connection has been established.
    ConnectionEstablished {
        id: ConnectionId,
        peer_id: PeerId,
        endpoint: ConnectedPoint,
        connection: NewConnection,
        /// [`Some`] when the new connection is an outgoing connection.
        /// Addresses are dialed in parallel. Contains the addresses and errors
        /// of dial attempts that failed before the one successful dial.
        concurrent_dial_errors: Option<Vec<(Multiaddr, TransportError<std::io::Error>)>>,
        /// How long it took to establish this connection.
        established_in: std::time::Duration,
    },

    /// An established connection was closed.
    ///
    /// A connection may close if
    ///
    ///   * it encounters an error, which includes the connection being
    ///     closed by the remote. In this case `error` is `Some`.
    ///   * it was actively closed by [`EstablishedConnection::start_close`],
    ///     i.e. a successful, orderly close.
    ///   * it was actively closed by [`Pool::disconnect`], i.e.
    ///     dropped without an orderly close.
    ///
    ConnectionClosed {
        id: ConnectionId,
        /// Information about the connection that errored.
        connected: Connected,
        /// The error that occurred, if any. If `None`, the connection
        /// was closed by the local peer.
        error: Option<ConnectionError>,
        /// The remaining established connections to the same peer.
        remaining_established_connection_ids: Vec<ConnectionId>,
    },

    /// An outbound connection attempt failed.
    PendingOutboundConnectionError {
        /// The ID of the failed connection.
        id: ConnectionId,
        /// The error that occurred.
        error: PendingOutboundConnectionError,
        /// The (expected) peer of the failed connection.
        peer: Option<PeerId>,
    },

    /// An inbound connection attempt failed.
    PendingInboundConnectionError {
        /// The ID of the failed connection.
        id: ConnectionId,
        /// Address used to send back data to the remote.
        send_back_addr: Multiaddr,
        /// Local connection address.
        local_addr: Multiaddr,
        /// The error that occurred.
        error: PendingInboundConnectionError,
    },

    /// A node has produced an event.
    ConnectionEvent {
        id: ConnectionId,
        peer_id: PeerId,
        /// The produced event.
        event: ToBehaviour,
    },

    /// The connection to a node has changed its address.
    AddressChange {
        id: ConnectionId,
        peer_id: PeerId,
        /// The new endpoint.
        new_endpoint: ConnectedPoint,
        /// The old endpoint.
        old_endpoint: ConnectedPoint,
    },
}

impl<THandler> Pool<THandler>
where
    THandler: ConnectionHandler,
{
    /// Creates a new empty `Pool`.
    pub(crate) fn new(local_id: PeerId, config: PoolConfig) -> Self {
        let (pending_connection_events_tx, pending_connection_events_rx) = mpsc::channel(0);
        let executor = match config.executor {
            Some(exec) => ExecSwitch::Executor(exec),
            None => ExecSwitch::LocalSpawn(Default::default()),
        };
        Pool {
            local_id,
            counters: ConnectionCounters::new(),
            established: Default::default(),
            pending: Default::default(),
            task_command_buffer_size: config.task_command_buffer_size,
            dial_concurrency_factor: config.dial_concurrency_factor,
            substream_upgrade_protocol_override: config.substream_upgrade_protocol_override,
            max_negotiating_inbound_streams: config.max_negotiating_inbound_streams,
            per_connection_event_buffer_size: config.per_connection_event_buffer_size,
            idle_connection_timeout: config.idle_connection_timeout,
            executor,
            pending_connection_events_tx,
            pending_connection_events_rx,
            no_established_connections_waker: None,
            established_connection_events: Default::default(),
            new_connection_dropped_listeners: Default::default(),
        }
    }

    /// Gets the dedicated connection counters.
    pub(crate) fn counters(&self) -> &ConnectionCounters {
        &self.counters
    }

    /// Gets an established connection from the pool by ID.
    pub(crate) fn get_established(
        &mut self,
        id: ConnectionId,
    ) -> Option<&mut EstablishedConnection<THandler::FromBehaviour>> {
        self.established
            .values_mut()
            .find_map(|connections| connections.get_mut(&id))
    }

    /// Returns true if we are connected to the given peer.
    ///
    /// This will return true only after a `NodeReached` event has been produced by `poll()`.
    pub(crate) fn is_connected(&self, id: PeerId) -> bool {
        self.established.contains_key(&id)
    }

    /// Returns the number of connected peers, i.e. those with at least one
    /// established connection in the pool.
    pub(crate) fn num_peers(&self) -> usize {
        self.established.len()
    }

    /// (Forcefully) close all connections to the given peer.
    ///
    /// All connections to the peer, whether pending or established are
    /// closed asap and no more events from these connections are emitted
    /// by the pool effective immediately.
    pub(crate) fn disconnect(&mut self, peer: PeerId) {
        if let Some(conns) = self.established.get_mut(&peer) {
            for (_, conn) in conns.iter_mut() {
                conn.start_close();
            }
        }

        for connection in self
            .pending
            .iter_mut()
            .filter_map(|(_, info)| info.is_for_same_remote_as(peer).then_some(info))
        {
            connection.abort()
        }
    }

    /// Returns an iterator over all established connections of `peer`.
    pub(crate) fn iter_established_connections_of_peer(
        &mut self,
        peer: &PeerId,
    ) -> impl Iterator<Item = ConnectionId> + '_ {
        match self.established.get(peer) {
            Some(conns) => either::Either::Left(conns.iter().map(|(id, _)| *id)),
            None => either::Either::Right(std::iter::empty()),
        }
    }

    /// Checks whether we are currently dialing the given peer.
    pub(crate) fn is_dialing(&self, peer: PeerId) -> bool {
        self.pending.iter().any(|(_, info)| {
            matches!(info.endpoint, PendingPoint::Dialer { .. }) && info.is_for_same_remote_as(peer)
        })
    }

    /// Returns an iterator over all connected peers, i.e. those that have
    /// at least one established connection in the pool.
    pub(crate) fn iter_connected(&self) -> impl Iterator<Item = &PeerId> {
        self.established.keys()
    }

    /// Adds a pending outgoing connection to the pool in the form of a `Future`
    /// that establishes and negotiates the connection.
    pub(crate) fn add_outgoing(
        &mut self,
        dials: Vec<
            BoxFuture<
                'static,
                (
                    Multiaddr,
                    Result<(PeerId, StreamMuxerBox), TransportError<std::io::Error>>,
                ),
            >,
        >,
        peer: Option<PeerId>,
        role_override: Endpoint,
        port_use: PortUse,
        dial_concurrency_factor_override: Option<NonZeroU8>,
        connection_id: ConnectionId,
    ) {
        let concurrency_factor =
            dial_concurrency_factor_override.unwrap_or(self.dial_concurrency_factor);
        let span = tracing::debug_span!(parent: tracing::Span::none(), "new_outgoing_connection", %concurrency_factor, num_dials=%dials.len(), id = %connection_id);
        span.follows_from(tracing::Span::current());

        let (abort_notifier, abort_receiver) = oneshot::channel();

        self.executor.spawn(
            task::new_for_pending_outgoing_connection(
                connection_id,
                ConcurrentDial::new(dials, concurrency_factor),
                abort_receiver,
                self.pending_connection_events_tx.clone(),
            )
            .instrument(span),
        );

        let endpoint = PendingPoint::Dialer {
            role_override,
            port_use,
        };

        self.counters.inc_pending(&endpoint);
        self.pending.insert(
            connection_id,
            PendingConnection {
                peer_id: peer,
                endpoint,
                abort_notifier: Some(abort_notifier),
                accepted_at: Instant::now(),
            },
        );
    }

    /// Adds a pending incoming connection to the pool in the form of a
    /// `Future` that establishes and negotiates the connection.
    pub(crate) fn add_incoming<TFut>(
        &mut self,
        future: TFut,
        info: IncomingInfo<'_>,
        connection_id: ConnectionId,
    ) where
        TFut: Future<Output = Result<(PeerId, StreamMuxerBox), std::io::Error>> + Send + 'static,
    {
        let endpoint = info.create_connected_point();

        let (abort_notifier, abort_receiver) = oneshot::channel();

        let span = tracing::debug_span!(parent: tracing::Span::none(), "new_incoming_connection", remote_addr = %info.send_back_addr, id = %connection_id);
        span.follows_from(tracing::Span::current());

        self.executor.spawn(
            task::new_for_pending_incoming_connection(
                connection_id,
                future,
                abort_receiver,
                self.pending_connection_events_tx.clone(),
            )
            .instrument(span),
        );

        self.counters.inc_pending_incoming();
        self.pending.insert(
            connection_id,
            PendingConnection {
                peer_id: None,
                endpoint: endpoint.into(),
                abort_notifier: Some(abort_notifier),
                accepted_at: Instant::now(),
            },
        );
    }

    pub(crate) fn spawn_connection(
        &mut self,
        id: ConnectionId,
        obtained_peer_id: PeerId,
        endpoint: &ConnectedPoint,
        connection: NewConnection,
        handler: THandler,
    ) {
        let connection = connection.extract();
        let conns = self.established.entry(obtained_peer_id).or_default();
        self.counters.inc_established(endpoint);

        let (command_sender, command_receiver) = mpsc::channel(self.task_command_buffer_size);
        let (event_sender, event_receiver) = mpsc::channel(self.per_connection_event_buffer_size);

        conns.insert(
            id,
            EstablishedConnection {
                endpoint: endpoint.clone(),
                sender: command_sender,
            },
        );
        self.established_connection_events.push(event_receiver);
        if let Some(waker) = self.no_established_connections_waker.take() {
            waker.wake();
        }

        let connection = Connection::new(
            connection,
            handler,
            self.substream_upgrade_protocol_override,
            self.max_negotiating_inbound_streams,
            self.idle_connection_timeout,
        );

        let span = tracing::debug_span!(parent: tracing::Span::none(), "new_established_connection", remote_addr = %endpoint.get_remote_address(), %id, peer = %obtained_peer_id);
        span.follows_from(tracing::Span::current());

        self.executor.spawn(
            task::new_for_established_connection(
                id,
                obtained_peer_id,
                connection,
                command_receiver,
                event_sender,
            )
            .instrument(span),
        )
    }

    /// Polls the connection pool for events.
    #[tracing::instrument(level = "debug", name = "Pool::poll", skip(self, cx))]
    pub(crate) fn poll(&mut self, cx: &mut Context<'_>) -> Poll<PoolEvent<THandler::ToBehaviour>>
    where
        THandler: ConnectionHandler + 'static,
        <THandler as ConnectionHandler>::OutboundOpenInfo: Send,
    {
        // Poll for events of established connections.
        //
        // Note that established connections are polled before pending connections, thus
        // prioritizing established connections over pending connections.
        match self.established_connection_events.poll_next_unpin(cx) {
            Poll::Pending => {}
            Poll::Ready(None) => {
                self.no_established_connections_waker = Some(cx.waker().clone());
            }

            Poll::Ready(Some(task::EstablishedConnectionEvent::Notify { id, peer_id, event })) => {
                return Poll::Ready(PoolEvent::ConnectionEvent { peer_id, id, event });
            }
            Poll::Ready(Some(task::EstablishedConnectionEvent::AddressChange {
                id,
                peer_id,
                new_address,
            })) => {
                let connection = self
                    .established
                    .get_mut(&peer_id)
                    .expect("Receive `AddressChange` event for established peer.")
                    .get_mut(&id)
                    .expect("Receive `AddressChange` event from established connection");
                let mut new_endpoint = connection.endpoint.clone();
                new_endpoint.set_remote_address(new_address);
                let old_endpoint =
                    std::mem::replace(&mut connection.endpoint, new_endpoint.clone());

                return Poll::Ready(PoolEvent::AddressChange {
                    peer_id,
                    id,
                    new_endpoint,
                    old_endpoint,
                });
            }
            Poll::Ready(Some(task::EstablishedConnectionEvent::Closed { id, peer_id, error })) => {
                let connections = self
                    .established
                    .get_mut(&peer_id)
                    .expect("`Closed` event for established connection");
                let EstablishedConnection { endpoint, .. } =
                    connections.remove(&id).expect("Connection to be present");
                self.counters.dec_established(&endpoint);
                let remaining_established_connection_ids: Vec<ConnectionId> =
                    connections.keys().cloned().collect();
                if remaining_established_connection_ids.is_empty() {
                    self.established.remove(&peer_id);
                }
                return Poll::Ready(PoolEvent::ConnectionClosed {
                    id,
                    connected: Connected { endpoint, peer_id },
                    error,
                    remaining_established_connection_ids,
                });
            }
        }

        // Poll for events of pending connections.
        loop {
            if let Poll::Ready(Some(result)) =
                self.new_connection_dropped_listeners.poll_next_unpin(cx)
            {
                if let Ok(dropped_connection) = result {
                    self.executor.spawn(async move {
                        let _ = dropped_connection.close().await;
                    });
                }
                continue;
            }

            let event = match self.pending_connection_events_rx.poll_next_unpin(cx) {
                Poll::Ready(Some(event)) => event,
                Poll::Pending => break,
                Poll::Ready(None) => unreachable!("Pool holds both sender and receiver."),
            };

            match event {
                task::PendingConnectionEvent::ConnectionEstablished {
                    id,
                    output: (obtained_peer_id, mut muxer),
                    outgoing,
                } => {
                    let PendingConnection {
                        peer_id: expected_peer_id,
                        endpoint,
                        abort_notifier: _,
                        accepted_at,
                    } = self
                        .pending
                        .remove(&id)
                        .expect("Entry in `self.pending` for previously pending connection.");

                    self.counters.dec_pending(&endpoint);

                    let (endpoint, concurrent_dial_errors) = match (endpoint, outgoing) {
                        (
                            PendingPoint::Dialer {
                                role_override,
                                port_use,
                            },
                            Some((address, errors)),
                        ) => (
                            ConnectedPoint::Dialer {
                                address,
                                role_override,
                                port_use,
                            },
                            Some(errors),
                        ),
                        (
                            PendingPoint::Listener {
                                local_addr,
                                send_back_addr,
                            },
                            None,
                        ) => (
                            ConnectedPoint::Listener {
                                local_addr,
                                send_back_addr,
                            },
                            None,
                        ),
                        (PendingPoint::Dialer { .. }, None) => unreachable!(
                            "Established incoming connection via pending outgoing connection."
                        ),
                        (PendingPoint::Listener { .. }, Some(_)) => unreachable!(
                            "Established outgoing connection via pending incoming connection."
                        ),
                    };

                    let check_peer_id = || {
                        if let Some(peer) = expected_peer_id {
                            if peer != obtained_peer_id {
                                return Err(PendingConnectionError::WrongPeerId {
                                    obtained: obtained_peer_id,
                                    endpoint: endpoint.clone(),
                                });
                            }
                        }

                        if self.local_id == obtained_peer_id {
                            return Err(PendingConnectionError::LocalPeerId {
                                endpoint: endpoint.clone(),
                            });
                        }

                        Ok(())
                    };

                    if let Err(error) = check_peer_id() {
                        self.executor.spawn(poll_fn(move |cx| {
                            if let Err(e) = ready!(muxer.poll_close_unpin(cx)) {
                                tracing::debug!(
                                    peer=%obtained_peer_id,
                                    connection=%id,
                                    "Failed to close connection to peer: {:?}",
                                    e
                                );
                            }
                            Poll::Ready(())
                        }));

                        match endpoint {
                            ConnectedPoint::Dialer { .. } => {
                                return Poll::Ready(PoolEvent::PendingOutboundConnectionError {
                                    id,
                                    error: error
                                        .map(|t| vec![(endpoint.get_remote_address().clone(), t)]),
                                    peer: expected_peer_id.or(Some(obtained_peer_id)),
                                })
                            }
                            ConnectedPoint::Listener {
                                send_back_addr,
                                local_addr,
                            } => {
                                return Poll::Ready(PoolEvent::PendingInboundConnectionError {
                                    id,
                                    error,
                                    send_back_addr,
                                    local_addr,
                                })
                            }
                        };
                    }

                    let established_in = accepted_at.elapsed();

                    let (connection, drop_listener) = NewConnection::new(muxer);
                    self.new_connection_dropped_listeners.push(drop_listener);

                    return Poll::Ready(PoolEvent::ConnectionEstablished {
                        peer_id: obtained_peer_id,
                        endpoint,
                        id,
                        connection,
                        concurrent_dial_errors,
                        established_in,
                    });
                }
                task::PendingConnectionEvent::PendingFailed { id, error } => {
                    if let Some(PendingConnection {
                        peer_id,
                        endpoint,
                        abort_notifier: _,
                        accepted_at: _, // Ignoring the time it took for the connection to fail.
                    }) = self.pending.remove(&id)
                    {
                        self.counters.dec_pending(&endpoint);

                        match (endpoint, error) {
                            (PendingPoint::Dialer { .. }, Either::Left(error)) => {
                                return Poll::Ready(PoolEvent::PendingOutboundConnectionError {
                                    id,
                                    error,
                                    peer: peer_id,
                                });
                            }
                            (
                                PendingPoint::Listener {
                                    send_back_addr,
                                    local_addr,
                                },
                                Either::Right(error),
                            ) => {
                                return Poll::Ready(PoolEvent::PendingInboundConnectionError {
                                    id,
                                    error,
                                    send_back_addr,
                                    local_addr,
                                });
                            }
                            (PendingPoint::Dialer { .. }, Either::Right(_)) => {
                                unreachable!("Inbound error for outbound connection.")
                            }
                            (PendingPoint::Listener { .. }, Either::Left(_)) => {
                                unreachable!("Outbound error for inbound connection.")
                            }
                        }
                    }
                }
            }
        }

        self.executor.advance_local(cx);

        Poll::Pending
    }
}

/// Opaque type for a new connection.
///
/// This connection has just been established but isn't part of the [`Pool`] yet.
/// It either needs to be spawned via [`Pool::spawn_connection`] or dropped if undesired.
///
/// On drop, this type send the connection back to the [`Pool`] where it will be gracefully closed.
#[derive(Debug)]
pub(crate) struct NewConnection {
    connection: Option<StreamMuxerBox>,
    drop_sender: Option<oneshot::Sender<StreamMuxerBox>>,
}

impl NewConnection {
    fn new(conn: StreamMuxerBox) -> (Self, oneshot::Receiver<StreamMuxerBox>) {
        let (sender, receiver) = oneshot::channel();

        (
            Self {
                connection: Some(conn),
                drop_sender: Some(sender),
            },
            receiver,
        )
    }

    fn extract(mut self) -> StreamMuxerBox {
        self.connection.take().unwrap()
    }
}

impl Drop for NewConnection {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            let _ = self
                .drop_sender
                .take()
                .expect("`drop_sender` to always be `Some`")
                .send(connection);
        }
    }
}

/// Network connection information.
#[derive(Debug, Clone)]
pub struct ConnectionCounters {
    /// The current number of incoming connections.
    pending_incoming: u32,
    /// The current number of outgoing connections.
    pending_outgoing: u32,
    /// The current number of established inbound connections.
    established_incoming: u32,
    /// The current number of established outbound connections.
    established_outgoing: u32,
}

impl ConnectionCounters {
    fn new() -> Self {
        Self {
            pending_incoming: 0,
            pending_outgoing: 0,
            established_incoming: 0,
            established_outgoing: 0,
        }
    }

    /// The total number of connections, both pending and established.
    pub fn num_connections(&self) -> u32 {
        self.num_pending() + self.num_established()
    }

    /// The total number of pending connections, both incoming and outgoing.
    pub fn num_pending(&self) -> u32 {
        self.pending_incoming + self.pending_outgoing
    }

    /// The number of incoming connections being established.
    pub fn num_pending_incoming(&self) -> u32 {
        self.pending_incoming
    }

    /// The number of outgoing connections being established.
    pub fn num_pending_outgoing(&self) -> u32 {
        self.pending_outgoing
    }

    /// The number of established incoming connections.
    pub fn num_established_incoming(&self) -> u32 {
        self.established_incoming
    }

    /// The number of established outgoing connections.
    pub fn num_established_outgoing(&self) -> u32 {
        self.established_outgoing
    }

    /// The total number of established connections.
    pub fn num_established(&self) -> u32 {
        self.established_outgoing + self.established_incoming
    }

    fn inc_pending(&mut self, endpoint: &PendingPoint) {
        match endpoint {
            PendingPoint::Dialer { .. } => {
                self.pending_outgoing += 1;
            }
            PendingPoint::Listener { .. } => {
                self.pending_incoming += 1;
            }
        }
    }

    fn inc_pending_incoming(&mut self) {
        self.pending_incoming += 1;
    }

    fn dec_pending(&mut self, endpoint: &PendingPoint) {
        match endpoint {
            PendingPoint::Dialer { .. } => {
                self.pending_outgoing -= 1;
            }
            PendingPoint::Listener { .. } => {
                self.pending_incoming -= 1;
            }
        }
    }

    fn inc_established(&mut self, endpoint: &ConnectedPoint) {
        match endpoint {
            ConnectedPoint::Dialer { .. } => {
                self.established_outgoing += 1;
            }
            ConnectedPoint::Listener { .. } => {
                self.established_incoming += 1;
            }
        }
    }

    fn dec_established(&mut self, endpoint: &ConnectedPoint) {
        match endpoint {
            ConnectedPoint::Dialer { .. } => {
                self.established_outgoing -= 1;
            }
            ConnectedPoint::Listener { .. } => {
                self.established_incoming -= 1;
            }
        }
    }
}

/// Configuration options when creating a [`Pool`].
///
/// The default configuration specifies no dedicated task executor, a
/// task event buffer size of 32, and a task command buffer size of 7.
pub(crate) struct PoolConfig {
    /// Executor to use to spawn tasks.
    pub(crate) executor: Option<Box<dyn Executor + Send>>,
    /// Size of the task command buffer (per task).
    pub(crate) task_command_buffer_size: usize,
    /// Size of the pending connection task event buffer and the established connection task event
    /// buffer.
    pub(crate) per_connection_event_buffer_size: usize,
    /// Number of addresses concurrently dialed for a single outbound connection attempt.
    pub(crate) dial_concurrency_factor: NonZeroU8,
    /// How long a connection should be kept alive once it is idling.
    pub(crate) idle_connection_timeout: Duration,
    /// The configured override for substream protocol upgrades, if any.
    substream_upgrade_protocol_override: Option<libp2p_core::upgrade::Version>,

    /// The maximum number of inbound streams concurrently negotiating on a connection.
    ///
    /// See [`Connection::max_negotiating_inbound_streams`].
    max_negotiating_inbound_streams: usize,
}

impl PoolConfig {
    pub(crate) fn new(executor: Option<Box<dyn Executor + Send>>) -> Self {
        Self {
            executor,
            task_command_buffer_size: 32,
            per_connection_event_buffer_size: 7,
            dial_concurrency_factor: NonZeroU8::new(8).expect("8 > 0"),
            idle_connection_timeout: Duration::ZERO,
            substream_upgrade_protocol_override: None,
            max_negotiating_inbound_streams: 128,
        }
    }

    /// Sets the maximum number of events sent to a connection's background task
    /// that may be buffered, if the task cannot keep up with their consumption and
    /// delivery to the connection handler.
    ///
    /// When the buffer for a particular connection is full, `notify_handler` will no
    /// longer be able to deliver events to the associated [`Connection`],
    /// thus exerting back-pressure on the connection and peer API.
    pub(crate) fn with_notify_handler_buffer_size(mut self, n: NonZeroUsize) -> Self {
        self.task_command_buffer_size = n.get() - 1;
        self
    }

    /// Sets the maximum number of buffered connection events (beyond a guaranteed
    /// buffer of 1 event per connection).
    ///
    /// When the buffer is full, the background tasks of all connections will stall.
    /// In this way, the consumers of network events exert back-pressure on
    /// the network connection I/O.
    pub(crate) fn with_per_connection_event_buffer_size(mut self, n: usize) -> Self {
        self.per_connection_event_buffer_size = n;
        self
    }

    /// Number of addresses concurrently dialed for a single outbound connection attempt.
    pub(crate) fn with_dial_concurrency_factor(mut self, factor: NonZeroU8) -> Self {
        self.dial_concurrency_factor = factor;
        self
    }

    /// Configures an override for the substream upgrade protocol to use.
    pub(crate) fn with_substream_upgrade_protocol_override(
        mut self,
        v: libp2p_core::upgrade::Version,
    ) -> Self {
        self.substream_upgrade_protocol_override = Some(v);
        self
    }

    /// The maximum number of inbound streams concurrently negotiating on a connection.
    ///
    /// See [`Connection::max_negotiating_inbound_streams`].
    pub(crate) fn with_max_negotiating_inbound_streams(mut self, v: usize) -> Self {
        self.max_negotiating_inbound_streams = v;
        self
    }
}
