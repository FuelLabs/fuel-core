//! Test module for validating TxUpdateStream state transitions.

#![allow(clippy::arithmetic_side_effects)]

use fuel_core_types::services::txpool::TransactionStatus;
use test_strategy::{
    proptest,
    Arbitrary,
};

use crate::{
    tests::utils,
    tx_status_stream::{
        State,
        TxStatusMessage,
        TxUpdateStream,
    },
};

/// Represents the possible state transitions in TxUpdateStream.
#[derive(Debug, PartialEq, Eq, Clone, Arbitrary)]
pub(crate) enum StateTransitions {
    AddMsg(#[strategy(utils::tx_status_message_strategy())] TxStatusMessage),
    AddFailure,
    CloseRecv,
    Next,
}

/// Returns the new state after applying the given `transition` to the current `state`.
pub(crate) fn validate_tx_update_stream_state(
    state: State,
    transition: StateTransitions,
) -> State {
    use State::*;
    use StateTransitions::*;
    match (state, transition) {
        (
            Empty,
            AddMsg(TxStatusMessage::Status(TransactionStatus::Submitted { time })),
        ) => Initial(TransactionStatus::Submitted { time }),
        // If not Submitted, it's an early success.
        (Empty, AddMsg(TxStatusMessage::Status(s))) => EarlySuccess(s),
        (Empty, AddMsg(TxStatusMessage::FailedStatus)) => Failed,
        (Empty, AddFailure) => Failed,
        (Empty | Initial(_), Next) => Empty,
        (Initial(s1), AddMsg(TxStatusMessage::Status(s2))) => Success(s1, s2),
        (Initial(s1), AddMsg(TxStatusMessage::FailedStatus)) => LateFailed(s1),
        (Initial(s), AddFailure) => LateFailed(s),
        (_, CloseRecv) => Closed,
        (EarlySuccess(_) | Failed | SenderClosed(_), Next) => Closed,
        (LateFailed(_), Next) => Failed,
        (Success(_, s2), Next) => SenderClosed(s2),
        // Final states.
        (Closed, _) => Closed,
        (EarlySuccess(s), _) => EarlySuccess(s),
        (Success(s1, s2), _) => Success(s1, s2),
        (Failed, _) => Failed,
        (LateFailed(s), _) => LateFailed(s),
        (SenderClosed(s), _) => SenderClosed(s),
    }
}

/// Proptest for validating TxUpdateStream state transitions.
#[proptest]
fn test_tx_update_stream_state(
    #[strategy(utils::state_strategy())] state: State,
    transition: StateTransitions,
) {
    let mut stream = TxUpdateStream::with_state(state.clone());
    let new_state = validate_tx_update_stream_state(state, transition.clone());
    match transition {
        StateTransitions::AddMsg(s) => {
            stream.add_msg(s);
        }
        StateTransitions::AddFailure => {
            stream.add_failure();
        }
        StateTransitions::CloseRecv => {
            stream.close_recv();
        }
        StateTransitions::Next => {
            stream.try_next();
        }
    }

    assert_eq!(new_state, *stream.state());
}
