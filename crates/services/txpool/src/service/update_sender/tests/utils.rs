use super::*;

pub fn transaction_status_strategy() -> impl Strategy<Value = TransactionStatus> {
    prop_oneof![
        Just(TransactionStatus::Submitted { time: Tai64(0) }),
        Just(TransactionStatus::Success {
            block_height: Default::default(),
            time: Tai64(0),
            result: None,
            receipts: vec![],
            total_gas: 0,
            total_fee: 0,
        }),
        Just(TransactionStatus::Failed {
            block_height: Default::default(),
            time: Tai64(0),
            result: None,
            receipts: vec![],
            total_gas: 0,
            total_fee: 0,
        }),
        Just(TransactionStatus::SqueezedOut {
            reason: Default::default(),
        }),
    ]
}

pub fn tx_update_strategy() -> impl Strategy<Value = TxUpdate> {
    let status = prop_oneof![
        tx_status_message_strategy(),
        Just(TxStatusMessage::FailedStatus),
    ];
    ((0..10u8).prop_map(|i| Bytes32::from([i; 32])), status)
        .prop_map(|(tx_id, message)| TxUpdate { tx_id, message })
}

pub fn tx_status_message_strategy() -> impl Strategy<Value = TxStatusMessage> {
    prop_oneof![
        transaction_status_strategy().prop_map(TxStatusMessage::Status),
        Just(TxStatusMessage::FailedStatus),
    ]
}

pub(super) fn state_strategy() -> impl Strategy<Value = State> {
    prop_oneof![
        Just(State::Empty),
        transaction_status_strategy().prop_map(State::Initial),
        transaction_status_strategy().prop_map(State::EarlySuccess),
        (transaction_status_strategy(), transaction_status_strategy())
            .prop_map(|(s1, s2)| State::Success(s1, s2)),
        transaction_status_strategy().prop_map(State::LateFailed),
        transaction_status_strategy().prop_map(State::SenderClosed),
        Just(State::Failed),
        Just(State::Closed),
    ]
}

pub(super) fn senders_strategy_all_ok(
) -> impl Strategy<Value = HashMap<Bytes32, Vec<Sender<(), MockSendStatus>>>> {
    senders_strategy(Just(TrySend::Ok))
}

pub(super) fn senders_strategy_any(
) -> impl Strategy<Value = HashMap<Bytes32, Vec<Sender<(), MockSendStatus>>>> {
    senders_strategy(any::<TrySend>())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Arbitrary)]
pub enum TrySend {
    Ok,
    Full,
    Closed,
}

pub(super) fn senders_strategy(
    try_send: impl Strategy<Value = TrySend>,
) -> impl Strategy<Value = HashMap<Bytes32, Vec<Sender<(), MockSendStatus>>>> {
    let s = (try_send, state_strategy()).prop_map(|(try_send, state)| {
        let mut tx = MockSendStatus::new();
        let is_closed = matches!(try_send, TrySend::Closed);
        let is_full = matches!(try_send, TrySend::Full);

        tx.expect_try_send().returning(move |_| match try_send {
            TrySend::Ok => Ok(()),
            TrySend::Full => Err(SendError::Full),
            TrySend::Closed => Err(SendError::Closed),
        });
        tx.expect_is_closed().returning(move || is_closed);
        tx.expect_is_full().returning(move || is_full);
        Sender {
            stream: TxUpdateStream::with_state(state),
            _permit: (),
            tx,
            created: Instant::now(),
        }
    });
    prop::collection::hash_map(
        (0..12u8).prop_map(|i| Bytes32::from([i; 32])),
        prop::collection::vec(s, 0..5),
        0..=10,
    )
}

#[derive(Clone)]
pub(super) struct SenderData {
    pub state: State,
    pub try_send: TrySend,
    pub is_closed: bool,
    pub is_full: bool,
}

impl SenderData {
    pub(super) fn ok(state: State) -> Self {
        Self {
            state,
            try_send: TrySend::Ok,
            is_closed: false,
            is_full: false,
        }
    }

    #[allow(dead_code)]
    pub(super) fn full(state: State) -> Self {
        Self {
            state,
            try_send: TrySend::Full,
            is_closed: false,
            is_full: true,
        }
    }

    pub(super) fn closed(state: State) -> Self {
        Self {
            state,
            try_send: TrySend::Closed,
            is_closed: true,
            is_full: false,
        }
    }

    pub fn empty_ok() -> Self {
        Self {
            state: State::Empty,
            try_send: TrySend::Ok,
            is_closed: false,
            is_full: false,
        }
    }
}

pub(super) fn construct_senders(
    keys: &[(u8, &[SenderData])],
) -> HashMap<Bytes32, Vec<Sender<(), MockSendStatus>>> {
    let mut senders = HashMap::new();
    for (i, states) in keys {
        let mut v = Vec::new();
        for SenderData {
            state,
            try_send,
            is_closed,
            is_full,
        } in states.iter().cloned()
        {
            let mut tx = MockSendStatus::new();
            tx.expect_try_send().returning(move |_| match try_send {
                TrySend::Ok => Ok(()),
                TrySend::Full => Err(SendError::Full),
                TrySend::Closed => Err(SendError::Closed),
            });
            tx.expect_is_closed().returning(move || is_closed);
            tx.expect_is_full().returning(move || is_full);
            v.push(Sender {
                stream: TxUpdateStream::with_state(state.clone()),
                _permit: (),
                tx,
                created: Instant::now(),
            });
        }
        senders.insert(Bytes32::from([*i; 32]), v);
    }
    senders
}

impl PermitTrait for () {}

impl Permits for () {
    fn try_acquire(self: Arc<Self>) -> Option<Permit> {
        Some(Permit::from(Box::new(())))
    }
}

pub(super) struct MockCreateChannel;

impl CreateChannel for MockCreateChannel {
    fn channel() -> (Tx, TxStatusStream) {
        let tx = Box::new(MockSendStatus::new());
        (tx, Box::pin(tokio_stream::pending()))
    }
}

#[allow(drop_bounds)]
pub(super) fn box_senders<
    T: PermitTrait + Send + Sync + 'static,
    U: SendStatus + Send + Sync + 'static,
>(
    senders: HashMap<Bytes32, Vec<Sender<T, U>>>,
) -> HashMap<Bytes32, Vec<Sender<Permit, Tx>>> {
    senders
        .into_iter()
        .map(|(k, v)| {
            (
                k,
                v.into_iter()
                    .map(|s| Sender {
                        _permit: Permit::from(Box::new(s._permit)),
                        stream: s.stream,
                        tx: Tx::from(Box::new(s.tx)),
                        created: s.created,
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<HashMap<_, _>>()
}
