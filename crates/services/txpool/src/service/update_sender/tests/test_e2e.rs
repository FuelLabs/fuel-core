use std::time::Duration;

use fuel_core_types::blockchain::primitives::BlockId;

use crate::service::update_sender::tests::test_sending::validate_send;

use super::*;

const MAX_CHANNELS: usize = 2;
const MAX_IDS: u8 = 2;
const CAPACITY: usize = 4;

#[derive(Debug, Clone, Arbitrary)]
enum Op {
    Send(
        #[strategy(0..MAX_IDS)] u8,
        #[strategy(utils::tx_status_message_strategy())] TxStatusMessage,
    ),
    Recv(#[strategy(0..MAX_CHANNELS)] usize),
    Subscribe(#[strategy(0..MAX_IDS)] u8),
    DropRecv(#[strategy(0..MAX_CHANNELS)] usize),
}

#[proptest]
fn test_update_sender(
    #[strategy(prop::collection::vec(Op::arbitrary(), 5..=6))] ops: Vec<Op>,
) {
    test_update_sender_inner(ops);
}

#[test]
fn test_update_sender_reg() {
    use Op::*;
    use TransactionStatus::*;
    use TxStatusMessage::*;

    let ops = vec![
        Subscribe(0),
        Send(
            0,
            Status(Success {
                block_id: BlockId::from([0; 32]),
                time: Tai64(0),
                result: None,
            }),
        ),
        Recv(0),
        Send(0, Status(Submitted { time: Tai64(0) })),
        Recv(0),
    ];
    test_update_sender_inner(ops);
}

fn test_update_sender_inner(ops: Vec<Op>) {
    thread_local! {
        static RT: tokio::runtime::Runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
    }
    let update = UpdateSender::new(CAPACITY);
    let mut receivers: Vec<TxStatusStream> = Vec::new();
    let mut model_receivers: Vec<(u8, usize, [Option<TxStatusMessage>; 2])> = Vec::new();
    let mut sender_id = 0usize;
    let mut model: HashMap<u8, HashMap<usize, State>> = HashMap::new();
    for op in ops {
        match op {
            Op::Send(id, s) => {
                update.send(TxUpdate {
                    tx_id: Bytes32::from([id; 32]),
                    message: s.clone(),
                });
                remove_closed(&mut model);
                model_send(&mut model, &mut model_receivers, id, s);
            }
            Op::Recv(i) => {
                let mut real_msg = None;
                if let Some(rx) = receivers.get_mut(i) {
                    RT.with(|rt| {
                        rt.block_on(async {
                            if let Ok(Some(msg)) =
                                tokio::time::timeout(Duration::from_millis(10), rx.next())
                                    .await
                            {
                                real_msg = Some(msg);
                            }
                        })
                    });
                }
                if let Some(rx) = model_receivers.get_mut(i) {
                    let msg = rx.2[1].take();
                    if msg.is_some() {
                        rx.2[1] = rx.2[0].clone();
                        rx.2[0] = None;
                    }
                    assert_eq!(
                        real_msg, msg,
                        "i: {}, real {:?}, model: {:?}",
                        i, real_msg, msg
                    );
                }
            }
            Op::Subscribe(id) => {
                if let Some(rx) =
                    update.try_subscribe::<MpscChannel>(Bytes32::from([id; 32]))
                {
                    receivers.push(rx);
                }

                remove_closed(&mut model);
                model_subscribe(&mut model, &mut model_receivers, &mut sender_id, id);
            }
            Op::DropRecv(i) => {
                if i < receivers.len() {
                    receivers.remove(i);
                }
                if i < model_receivers.len() {
                    model_receivers.remove(i);
                }
            }
        }
    }
}

fn remove_closed(model: &mut HashMap<u8, HashMap<usize, State>>) {
    model.retain(|_, senders| {
        senders.retain(|_, state| !matches!(state, State::Closed));
        !senders.is_empty()
    });
}

fn model_subscribe(
    model: &mut HashMap<u8, HashMap<usize, State>>,
    model_receivers: &mut Vec<(u8, usize, [Option<TxStatusMessage>; 2])>,
    sender_id: &mut usize,
    id: u8,
) {
    if model.values().map(|v| v.len()).sum::<usize>() < CAPACITY {
        let senders = model.entry(id).or_insert_with(HashMap::new);
        senders.insert(*sender_id, State::Empty);

        model_receivers.push((id, *sender_id, [None, None]));
        *sender_id += 1;
    }
}

fn model_send(
    model: &mut HashMap<u8, HashMap<usize, State>>,
    model_receivers: &mut [(u8, usize, [Option<TxStatusMessage>; 2])],
    id: u8,
    s: TxStatusMessage,
) {
    if let Some(senders) = model.get_mut(&id) {
        let mut to_remove = Vec::new();
        for (i, sender) in senders.iter_mut() {
            let buf = model_receivers.iter_mut().find_map(|(key, index, buf)| {
                (*key == id && *index == *i).then_some(buf)
            });
            let tx = match buf {
                Some(buf) => match buf {
                    [None, None] => {
                        buf[1] = Some(s.clone());
                        Ok(())
                    }
                    [None, Some(_)] => {
                        buf[0] = Some(s.clone());
                        Ok(())
                    }
                    [Some(_), Some(_)] => Err(SendError::Full),
                    [Some(_), None] => unreachable!(),
                },
                None => Err(SendError::Closed),
            };

            *sender = validate_send(tx, sender.clone(), s.clone());
            if matches!(sender, State::Closed) {
                to_remove.push(*i);
            }
        }
        for i in to_remove {
            senders.remove(&i);
        }
    }
}
