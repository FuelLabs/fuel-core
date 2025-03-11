//! Test module for validating TxUpdateStream state transitions.

#![allow(clippy::arithmetic_side_effects)]

use test_strategy::proptest;

use crate::{
    tests::{
        tests_e2e::apply_tx_state_transition,
        utils,
    },
    tx_status_stream::{
        State,
        TxUpdateStream,
    },
};

use super::tests_e2e::StateTransitions;

/// Proptest for validating TxUpdateStream state transitions.
#[proptest]
fn test_tx_update_stream_state(
    #[strategy(utils::state_strategy())] state: State,
    transition: StateTransitions,
) {
    let mut stream = TxUpdateStream::with_state(state.clone());
    let new_state = apply_tx_state_transition(state, transition.clone());
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
