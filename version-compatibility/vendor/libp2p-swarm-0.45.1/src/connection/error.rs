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

use crate::transport::TransportError;
use crate::Multiaddr;
use crate::{ConnectedPoint, PeerId};
use std::{fmt, io};

/// Errors that can occur in the context of an established `Connection`.
#[derive(Debug)]
pub enum ConnectionError {
    /// An I/O error occurred on the connection.
    // TODO: Eventually this should also be a custom error?
    IO(io::Error),

    /// The connection keep-alive timeout expired.
    KeepAliveTimeout,
}

impl fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectionError::IO(err) => write!(f, "Connection error: I/O error: {err}"),
            ConnectionError::KeepAliveTimeout => {
                write!(f, "Connection closed due to expired keep-alive timeout.")
            }
        }
    }
}

impl std::error::Error for ConnectionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConnectionError::IO(err) => Some(err),
            ConnectionError::KeepAliveTimeout => None,
        }
    }
}

impl From<io::Error> for ConnectionError {
    fn from(error: io::Error) -> Self {
        ConnectionError::IO(error)
    }
}

/// Errors that can occur in the context of a pending outgoing `Connection`.
///
/// Note: Addresses for an outbound connection are dialed in parallel. Thus, compared to
/// [`PendingInboundConnectionError`], one or more [`TransportError`]s can occur for a single
/// connection.
pub(crate) type PendingOutboundConnectionError =
    PendingConnectionError<Vec<(Multiaddr, TransportError<io::Error>)>>;

/// Errors that can occur in the context of a pending incoming `Connection`.
pub(crate) type PendingInboundConnectionError = PendingConnectionError<TransportError<io::Error>>;

/// Errors that can occur in the context of a pending `Connection`.
#[derive(Debug)]
pub enum PendingConnectionError<TTransErr> {
    /// An error occurred while negotiating the transport protocol(s) on a connection.
    Transport(TTransErr),

    /// Pending connection attempt has been aborted.
    Aborted,

    /// The peer identity obtained on the connection did not
    /// match the one that was expected.
    WrongPeerId {
        obtained: PeerId,
        endpoint: ConnectedPoint,
    },

    /// The connection was dropped because it resolved to our own [`PeerId`].
    LocalPeerId { endpoint: ConnectedPoint },
}

impl<T> PendingConnectionError<T> {
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> PendingConnectionError<U> {
        match self {
            PendingConnectionError::Transport(t) => PendingConnectionError::Transport(f(t)),
            PendingConnectionError::Aborted => PendingConnectionError::Aborted,
            PendingConnectionError::WrongPeerId { obtained, endpoint } => {
                PendingConnectionError::WrongPeerId { obtained, endpoint }
            }
            PendingConnectionError::LocalPeerId { endpoint } => {
                PendingConnectionError::LocalPeerId { endpoint }
            }
        }
    }
}

impl<TTransErr> fmt::Display for PendingConnectionError<TTransErr>
where
    TTransErr: fmt::Display + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PendingConnectionError::Aborted => write!(f, "Pending connection: Aborted."),
            PendingConnectionError::Transport(err) => {
                write!(
                    f,
                    "Pending connection: Transport error on connection: {err}"
                )
            }
            PendingConnectionError::WrongPeerId { obtained, endpoint } => {
                write!(
                    f,
                    "Pending connection: Unexpected peer ID {obtained} at {endpoint:?}."
                )
            }
            PendingConnectionError::LocalPeerId { endpoint } => {
                write!(f, "Pending connection: Local peer ID at {endpoint:?}.")
            }
        }
    }
}

impl<TTransErr> std::error::Error for PendingConnectionError<TTransErr>
where
    TTransErr: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PendingConnectionError::Transport(_) => None,
            PendingConnectionError::WrongPeerId { .. } => None,
            PendingConnectionError::LocalPeerId { .. } => None,
            PendingConnectionError::Aborted => None,
        }
    }
}
