use std::sync::{
    atomic,
    atomic::AtomicBool,
    Arc,
};

use super::*;
use crate::tx_pool::TransactionStatus;
use fuel_core_interfaces::common::tai64::Tai64;
use test_case::test_case;

struct Input<I>
where
    I: Iterator<Item = anyhow::Result<Option<TransactionStatus>>> + Send + 'static,
{
    requested_id: Bytes32,
    status_updates: Vec<Result<TxUpdate, BroadcastStreamRecvError>>,
    db_statuses: I,
}

#[derive(Debug, PartialEq, Eq)]
enum Expected {
    Received(Vec<TransactionStatus>),
    TimedOut(Vec<TransactionStatus>),
    Error,
}

fn txn_id(i: u8) -> Bytes32 {
    [i; 32].into()
}

fn txn_updated(tx_id: Bytes32) -> TxUpdate {
    TxUpdate::updated(tx_id)
}

fn txn_squeezed(tx_id: Bytes32) -> TxUpdate {
    TxUpdate::squeezed_out(tx_id, fuel_txpool::Error::SqueezedOut(String::new()))
}

fn db_always_some(
    mut f: impl FnMut() -> TransactionStatus,
) -> impl Iterator<Item = anyhow::Result<Option<TransactionStatus>>> {
    std::iter::repeat_with(move || Ok(Some(f())))
}

fn db_always_none() -> impl Iterator<Item = anyhow::Result<Option<TransactionStatus>>> {
    std::iter::repeat_with(|| Ok(None))
}

fn db_always_error(
    mut f: impl FnMut() -> anyhow::Error,
) -> impl Iterator<Item = anyhow::Result<Option<TransactionStatus>>> {
    std::iter::repeat_with(move || Err(f()))
}

fn db_some(
    f: Vec<TransactionStatus>,
) -> impl Iterator<Item = anyhow::Result<Option<TransactionStatus>>> {
    f.into_iter().map(|t| Ok(Some(t)))
}

fn db_none(n: usize) -> impl Iterator<Item = anyhow::Result<Option<TransactionStatus>>> {
    (0..n).map(|_| Ok(None))
}

fn db_error(
    f: Vec<anyhow::Error>,
) -> impl Iterator<Item = anyhow::Result<Option<TransactionStatus>>> {
    f.into_iter().map(Err)
}

fn submitted() -> TransactionStatus {
    TransactionStatus::Submitted { time: Tai64(0) }
}

fn success() -> TransactionStatus {
    TransactionStatus::Success {
        block_id: Default::default(),
        time: Tai64(0),
        result: None,
    }
}

fn failed() -> TransactionStatus {
    TransactionStatus::Failed {
        block_id: Default::default(),
        time: Tai64(0),
        result: None,
        reason: Default::default(),
    }
}

fn squeezed() -> TransactionStatus {
    TransactionStatus::SqueezedOut {
        reason: fuel_txpool::Error::SqueezedOut(String::new()).to_string(),
    }
}

#[test_case(
    Input {
        requested_id: txn_id(2),
        status_updates: vec![],
        db_statuses: db_always_none(),
    }
    => Expected::TimedOut(vec![])
    ; "no status update, no db status, times out"
)]
#[test_case(
    Input {
        requested_id: txn_id(2),
        status_updates: vec![Ok(txn_updated(txn_id(3)))],
        db_statuses: db_always_none(),
    }
    => Expected::TimedOut(vec![])
    ; "unrelated status update, no db status, times out"
)]
#[test_case(
    Input {
        requested_id: txn_id(2),
        status_updates: vec![Ok(txn_updated(txn_id(2)))],
        db_statuses: db_always_none(),
    }
    => Expected::Received(vec![])
    ; "status update, no db status, receives no status"
)]
#[test_case(
    Input {
        requested_id: txn_id(2),
        status_updates: vec![Ok(txn_updated(txn_id(2)))],
        db_statuses: db_none(1).chain(db_always_some(submitted)),
    }
    => Expected::TimedOut(vec![submitted()])
    ; "submitted update, submitted db status, times out with one submitted status"
)]
#[test_case(
    Input {
        requested_id: txn_id(2),
        status_updates: vec![Ok(txn_updated(txn_id(2)))],
        db_statuses: db_none(1).chain(db_always_error(|| anyhow::anyhow!("db failed"))),
    }
    => Expected::Error
    ; "status update, none then error db status, gets error"
)]
#[test_case(
    Input {
        requested_id: txn_id(2),
        status_updates: vec![],
        db_statuses: db_error(vec![anyhow::anyhow!("db failed")]),
    }
    => Expected::Error
    ; "no status update, error db status, gets error"
)]
#[test_case(
    Input {
        requested_id: txn_id(2),
        status_updates: vec![Err(BroadcastStreamRecvError::Lagged(1))],
        db_statuses: db_always_none(),
    }
    => Expected::TimedOut(vec![])
    ; "lagged status update, no db status, times out with no results"
)]
#[test_case(
    Input {
        requested_id: txn_id(2),
        status_updates: vec![Ok(txn_updated(txn_id(2))), Ok(txn_updated(txn_id(2)))],
        db_statuses: db_none(1).chain(db_some(vec![submitted()])).chain(db_always_some(success)),
    }
    => Expected::Received(vec![submitted(), success()])
    ; "updated then updated status, submitted then success db status, received submitted then success status"
)]
#[test_case(
    Input {
        requested_id: txn_id(2),
        status_updates: vec![Ok(txn_updated(txn_id(20))), Ok(txn_updated(txn_id(8)))],
        db_statuses: db_none(1),
    }
    => Expected::TimedOut(vec![])
    ; "unrelated status updates, no db status, times out with no status"
)]
#[test_case(
    Input {
        requested_id: txn_id(2),
        status_updates: vec![Ok(txn_updated(txn_id(2))), Ok(txn_updated(txn_id(2)))],
        db_statuses: db_none(1).chain(db_some(vec![submitted()])).chain(db_always_some(failed)),
    }
    => Expected::Received(vec![submitted(), failed()])
    ; "updated then updated status, submitted then failed db status, received submitted then failed status"
)]
#[test_case(
    Input {
        requested_id: txn_id(2),
        status_updates: vec![],
        db_statuses: db_some(vec![success()]),
    }
    => Expected::Received(vec![success()])
    ; "no status update, success db status, received success status"
)]
#[test_case(
    Input {
        requested_id: txn_id(2),
        status_updates: vec![],
        db_statuses: db_some(vec![failed()]),
    }
    => Expected::Received(vec![failed()])
    ; "no status update, failed db status, received failed status"
)]
#[test_case(
    Input {
        requested_id: txn_id(2),
        status_updates: vec![Ok(txn_squeezed(txn_id(2)))],
        db_statuses: db_none(1),
    }
    => Expected::Received(vec![squeezed()])
    ; "squeezed status updates, no db status, received squeezed status"
)]
#[test_case(
    Input {
        requested_id: txn_id(2),
        status_updates: vec![Ok(txn_updated(txn_id(2))), Ok(txn_squeezed(txn_id(2)))],
        db_statuses: db_none(1).chain(db_some(vec![submitted()])).chain(db_none(1)),
    }
    => Expected::Received(vec![submitted(), squeezed()])
    ; "updated then squeezed status, db is set to submitted then nothing, received submitted squeezed"
)]
#[tokio::test]
async fn create_tx_status_change_stream<I>(input: Input<I>) -> Expected
where
    I: Iterator<Item = anyhow::Result<Option<TransactionStatus>>> + Send + 'static,
{
    let Input {
        requested_id: transaction_id,
        status_updates,
        mut db_statuses,
    } = input;
    let mut state = MockTxnStatusChangeState::new();
    state
        .expect_get_tx_status()
        .returning(move |_| db_statuses.next().unwrap().map(|t| t.map(|t| t.into())));
    let state = Box::new(state);
    let ids = status_updates.to_vec();
    let reached_end = Arc::new(AtomicBool::new(false));
    let re = reached_end.clone();
    let stream = futures::stream::iter(ids)
        .chain(futures::stream::once(async move {
            re.store(true, atomic::Ordering::SeqCst);
            Ok(txn_updated(txn_id(255)))
        }))
        .boxed();

    let stream = transaction_status_change(state, stream, transaction_id).await;
    let r: Result<Vec<_>, _> = stream.try_collect().await;
    let timeout = reached_end.load(atomic::Ordering::SeqCst);
    match r {
        Ok(r) => {
            let r = r.into_iter().map(|t| t.into()).collect();
            if timeout {
                Expected::TimedOut(r)
            } else {
                Expected::Received(r)
            }
        }
        Err(_) => Expected::Error,
    }
}
