use std::sync::Arc;

use fuel_core_types::blockchain::primitives::BlockId;

use crate::service::update_sender::tests::utils::{
    construct_senders,
    SenderData,
};

use super::{
    utils::{
        box_senders,
        senders_strategy_any,
        tx_update_strategy,
    },
    *,
};

pub(super) fn validate_send(
    tx: Result<(), SendError>,
    state: State,
    msg: TxStatusMessage,
) -> State {
    let state = validate_tx_update_stream_state(state, StateTransitions::AddMsg(msg));
    let state = validate_tx_update_stream_state(state, StateTransitions::Next);
    match tx {
        Ok(()) => state,
        Err(SendError::Closed) => {
            validate_tx_update_stream_state(state, StateTransitions::CloseRecv)
        }
        Err(SendError::Full) => {
            validate_tx_update_stream_state(state, StateTransitions::AddFailure)
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
        message: TxStatusMessage::Status(TransactionStatus::Success {
            block_id: BlockId::from([0; 32]),
            time: Tai64(0),
            result: None,
        }),
    };
    test_send_inner(
        update,
        construct_senders(&[(
            2,
            &[
                SenderData::closed(Success(
                    TransactionStatus::Submitted { time: Tai64(0) },
                    TransactionStatus::Submitted { time: Tai64(0) },
                )),
                SenderData::ok(Initial(TransactionStatus::Submitted { time: Tai64(0) })),
            ],
        )]),
    );
}

fn test_send_inner(
    msg: TxUpdate,
    senders: HashMap<Bytes32, Vec<Sender<(), MockSendStatus>>>,
) {
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
            .filter(|(_, state)| {
                !matches!(
                    state,
                    State::Closed | State::EarlySuccess(_) | State::Failed
                )
            })
            .collect::<Vec<_>>()
    });
    let update = UpdateSender {
        senders: Arc::new(Mutex::new(box_senders(senders))),
        permits: Arc::new(()),
    };
    update.send(msg.clone());
    if let Some(before) = before {
        let lock = update.senders.lock();
        if let Some(senders) = lock.get(&msg.tx_id) {
            let mut i = 0;
            for (tx, state) in before {
                let new_state = validate_send(tx, state, msg.message.clone());
                if matches!(new_state, State::Closed) {
                    continue
                }
                assert_eq!(*senders[i].stream.state(), new_state);
                i += 1;
            }
        }
    }
}
