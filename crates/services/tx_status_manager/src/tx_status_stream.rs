use fuel_core_types::{
    fuel_tx::Bytes32,
    services::txpool::TransactionStatus,
};
use std::pin::Pin;
use tokio_stream::Stream;

#[derive(Debug, Clone)]
pub struct TxUpdate {
    pub(crate) tx_id: Bytes32,
    pub(crate) message: TxStatusMessage,
}

impl TxUpdate {
    pub fn new(tx_id: Bytes32, message: TxStatusMessage) -> Self {
        Self { tx_id, message }
    }

    pub fn tx_id(&self) -> &Bytes32 {
        &self.tx_id
    }

    pub fn into_msg(self) -> TxStatusMessage {
        self.message
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TxStatusMessage {
    Status(TransactionStatus),
    FailedStatus,
}

impl TxStatusMessage {
    pub fn is_final(&self) -> bool {
        match self {
            TxStatusMessage::Status(transaction_status) => transaction_status.is_final(),
            TxStatusMessage::FailedStatus => true,
        }
    }
}

impl<E> From<Result<TransactionStatus, E>> for TxStatusMessage {
    fn from(result: Result<TransactionStatus, E>) -> Self {
        match result {
            Ok(status) => TxStatusMessage::Status(status),
            Err(_) => TxStatusMessage::FailedStatus,
        }
    }
}
impl From<TransactionStatus> for TxStatusMessage {
    fn from(status: TransactionStatus) -> Self {
        TxStatusMessage::Status(status)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum State {
    Empty,
    // The transaction has been submitted to the txpool
    Submitted(TransactionStatus),
    // The transaction has been preconfirmed by execution
    Preconfirmed(TransactionStatus),
    // We received a final status for the transaction without receiving anything before
    EarlySuccess(TransactionStatus),
    // We received a final status for the transaction after receiving an other status first
    Success(TransactionStatus, TransactionStatus),
    // We received a failed status
    Failed,
    // We received a failed status after receiving an other status first
    LateFailed(TransactionStatus),
    SenderClosed(TransactionStatus),
    Closed,
}

#[derive(Debug)]
pub struct TxUpdateStream {
    state: State,
}

impl TxUpdateStream {
    pub fn new() -> Self {
        Self {
            state: State::Empty,
        }
    }

    #[cfg(test)]
    pub(super) fn with_state(state: State) -> Self {
        Self { state }
    }

    #[cfg(test)]
    pub(super) fn state(&self) -> &State {
        &self.state
    }

    pub fn add_msg(&mut self, msg: TxStatusMessage) {
        let state = std::mem::replace(&mut self.state, State::Empty);
        self.state = match state {
            State::Empty => match msg {
                TxStatusMessage::Status(TransactionStatus::Submitted(s)) => {
                    State::Submitted(TransactionStatus::Submitted(s))
                }

                TxStatusMessage::Status(TransactionStatus::PreConfirmationSuccess(s)) => {
                    State::Preconfirmed(TransactionStatus::PreConfirmationSuccess(s))
                }
                TxStatusMessage::Status(TransactionStatus::PreConfirmationFailure(s)) => {
                    State::Preconfirmed(TransactionStatus::PreConfirmationFailure(s))
                }

                TxStatusMessage::Status(s) => State::EarlySuccess(s),

                TxStatusMessage::FailedStatus => State::Failed,
            },
            State::Submitted(s1) => match msg {
                TxStatusMessage::Status(TransactionStatus::Submitted(s2)) => {
                    State::Submitted(TransactionStatus::Submitted(s2))
                }

                TxStatusMessage::Status(TransactionStatus::PreConfirmationSuccess(
                    s2,
                )) => State::Preconfirmed(TransactionStatus::PreConfirmationSuccess(s2)),
                TxStatusMessage::Status(TransactionStatus::PreConfirmationFailure(
                    s2,
                )) => State::Preconfirmed(TransactionStatus::PreConfirmationFailure(s2)),

                TxStatusMessage::Status(s2) => State::Success(s1, s2),

                TxStatusMessage::FailedStatus => State::LateFailed(s1),
            },
            State::Preconfirmed(s1) => {
                if let TxStatusMessage::Status(s2) = msg {
                    State::Success(s1, s2)
                } else {
                    State::LateFailed(s1)
                }
            }
            s => s,
        };
    }

    pub fn add_failure(&mut self) {
        let state = std::mem::replace(&mut self.state, State::Empty);
        self.state = match state {
            State::Submitted(s) | State::Preconfirmed(s) => State::LateFailed(s),
            State::Empty => State::Failed,
            s => s,
        };
    }

    pub fn close_recv(&mut self) {
        self.state = State::Closed;
    }

    pub fn try_next(&mut self) -> Option<TxStatusMessage> {
        let state = std::mem::replace(&mut self.state, State::Empty);
        match state {
            State::Submitted(s) => Some(TxStatusMessage::Status(s)),
            State::Preconfirmed(s) => Some(TxStatusMessage::Status(s)),
            State::Empty => None,
            State::EarlySuccess(s) | State::SenderClosed(s) => {
                self.state = State::Closed;
                Some(TxStatusMessage::Status(s))
            }
            State::Failed => {
                self.state = State::Closed;
                Some(TxStatusMessage::FailedStatus)
            }
            State::LateFailed(s) => {
                self.state = State::Failed;
                Some(TxStatusMessage::Status(s))
            }
            State::Success(s1, s2) => {
                self.state = State::SenderClosed(s2);
                Some(TxStatusMessage::Status(s1))
            }
            State::Closed => {
                self.state = State::Closed;
                None
            }
        }
    }

    pub fn is_closed(&self) -> bool {
        matches!(self.state, State::Closed)
    }
}

pub type TxStatusStream = Pin<Box<dyn Stream<Item = TxStatusMessage> + Send + Sync>>;
