use std::{
    collections::HashMap,
    sync::Arc,
    time::Duration,
};

use fuel_core_types::{
    fuel_tx::Bytes32,
    services::txpool::TransactionStatus,
};
use parking_lot::lock_api::Mutex;
use test_strategy::proptest;

use crate::{
    tests::utils::{
        box_senders,
        construct_senders,
        senders_strategy_any,
        tx_update_strategy,
        SenderData,
    },
    tx_status_stream::{
        State,
        TxStatusMessage,
        TxUpdate,
    },
    update_sender::{
        MockSendStatus,
        SendError,
        SendStatus,
        Sender,
        UpdateSender,
    },
};

use super::tests_e2e::{
    apply_tx_state_transition,
    StateTransitions,
};

/// Model the function that sends a message to the receiver.
pub(super) fn validate_send(
    tx: Result<(), SendError>,
    state: State,
    msg: TxStatusMessage,
) -> State {
    // Add the message to the stream.
    let state = apply_tx_state_transition(state, StateTransitions::AddMsg(msg));

    // Try to get the next message from the stream.
    let state = apply_tx_state_transition(state, StateTransitions::Next);

    // Try to send the message to the receiver.
    match tx {
        // If ok, then use this state.
        Ok(()) => state,
        // If the receiver is closed, then update the state.
        Err(SendError::Closed) => {
            apply_tx_state_transition(state, StateTransitions::CloseRecv)
        }
        // If the receiver is full, then update the state.
        Err(SendError::Full) => {
            apply_tx_state_transition(state, StateTransitions::AddFailure)
        }
    }
}

#[proptest]
fn test_send(
    #[strategy(tx_update_strategy())] update: TxUpdate,
    #[strategy(senders_strategy_any())] senders: HashMap<
        Bytes32,
        Vec<Sender<(), MockSendStatus>>,
    >,
) {
    test_send_inner(update, senders);
}

#[test]
fn test_send_reg() {
    use State::*;
    let update = TxUpdate {
        tx_id: Bytes32::from([2; 32]),
        message: TxStatusMessage::Status(TransactionStatus::Success(Default::default())),
    };
    test_send_inner(
        update,
        construct_senders(&[(
            2,
            &[
                SenderData::closed(Success(
                    TransactionStatus::Submitted(Default::default()),
                    TransactionStatus::Submitted(Default::default()),
                )),
                SenderData::ok(Submitted(TransactionStatus::Submitted(
                    Default::default(),
                ))),
            ],
        )]),
    );
}

/// Test the sending behavior of messages.
///
/// This function helps to simulate the sending process of a message.
/// It ensures that the message is sent to the expected recipients, and
/// validates its new state after being sent.
fn test_send_inner(
    msg: TxUpdate,
    senders: HashMap<Bytes32, Vec<Sender<(), MockSendStatus>>>,
) {
    // Get senders with a valid state, mapping to a tuple (msg send result, stream state)
    let before = senders.get(&msg.tx_id).map(|senders| {
        senders
            .iter()
            .map(|sender| {
                let tx = if sender.tx.is_full() {
                    Err(SendError::Full)
                } else if sender.tx.is_closed() {
                    Err(SendError::Closed)
                } else {
                    Ok(())
                };
                (tx, sender.stream.state().clone())
            })
            // Filter out closed senders or senders that will close.
            .filter(|(_, state)| {
                !matches!(
                    state,
                    State::Closed | State::EarlySuccess(_) | State::Failed
                )
            })
            .collect::<Vec<_>>()
    });

    // Create an UpdateSender and send the message
    let update = UpdateSender {
        senders: Arc::new(Mutex::new(box_senders(senders))),
        permits: Arc::new(()),
        ttl: Duration::from_secs(5),
    };
    let message = msg.message.clone();
    let tx_id = msg.tx_id;
    update.send(msg);

    // If there were valid senders, validate the message send status
    if let Some(before) = before {
        let lock = update.senders.lock();
        if let Some(senders) = lock.get(&tx_id) {
            let mut i = 0;
            for (tx, state) in before {
                // Validate the send result and get the new state
                let new_state = validate_send(tx, state, message.clone());

                // If the new state is closed, skip validation
                if matches!(new_state, State::Closed) {
                    continue
                }

                // Verify the new state of the sender
                assert_eq!(*senders[i].stream.state(), new_state);
                i = i.saturating_add(1);
            }
        }
    }
}
