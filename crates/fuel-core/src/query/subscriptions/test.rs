// Allow `arc_with_non_send_sync` to enable use of `Arbitrary`
#![allow(clippy::arc_with_non_send_sync)]

//! This module provides functions and data structures for
//! testing and modeling transaction status changes. Property-based
//! testing techniques to generate different
//! transaction status scenarios and validate the behavior of the
//! tested functions.
//!
//! The search space is kept small using strategies to constrain the inputs.
//!
//! The module defines several types, including:
//! - `TxStatus`: Represents the possible transaction status values, including Submitted and Final statuses
//! - `FinalTxStatus`: Represents the final transaction status values (Success, Squeezed, and Failed)
//!
//! The module also provides strategies for generating test data values:
//! - `state()`: Generates an Option<TransactionStatus>
//! - `state_result()`: Generates a Result<Option<TransactionStatus>, Error>
//! - `tx_status_message()`: Generates a TxStatusMessage
//! - `transaction_status()`: Generates a TransactionStatus
//! - `input_stream()`: Generates a Vec<TxStatusMessage> of length 0 to 5
use fuel_core_txpool::service::TxStatusMessage;
use fuel_core_types::{
    fuel_types::Bytes32,
    services::txpool::TransactionStatus,
    tai64::Tai64,
};
use futures::StreamExt;
use std::ops::ControlFlow;

use fuel_core_storage::Error as StorageError;
use proptest::{
    prelude::prop,
    prop_oneof,
    strategy::{
        Just,
        Strategy,
    },
};
use test_strategy::*;

/// Returns a Transaction ID value from a u8
fn txn_id(i: u8) -> Bytes32 {
    [i; 32].into()
}

/// Returns a TransactionStatus with Submitted status and time set to 0
fn submitted() -> TransactionStatus {
    TransactionStatus::Submitted { time: Tai64(0) }
}

/// Returns a TransactionStatus with Success status, time set to 0, and result set to None
fn success() -> TransactionStatus {
    TransactionStatus::Success {
        block_height: Default::default(),
        time: Tai64(0),
        result: None,
        receipts: vec![],
        total_gas: 0,
        total_fee: 0,
    }
}

/// Returns a TransactionStatus with Failed status, time set to 0, result set to None, and empty reason
fn failed() -> TransactionStatus {
    TransactionStatus::Failed {
        block_height: Default::default(),
        time: Tai64(0),
        result: None,
        receipts: vec![],
        total_gas: 0,
        total_fee: 0,
    }
}

/// Returns a TransactionStatus with SqueezedOut status and an empty error message
fn squeezed() -> TransactionStatus {
    TransactionStatus::SqueezedOut {
        reason: fuel_core_txpool::Error::SqueezedOut(String::new()).to_string(),
    }
}

/// Represents the different status that a transaction can have.
/// Submitted represents the initial status of the transaction,
/// in which it has been sent to the txpool but has not yet been included into a block.
/// Final indicates that the transaction has reached one of the final statuses (Success, Squeezed, or Failed).
#[derive(Debug, Clone, PartialEq, Eq, Arbitrary)]
enum TxStatus {
    /// The transaction has been submitted
    Submitted,
    /// The transaction has reached a final status
    Final(FinalTxStatus),
}

/// Represents the final transaction statuses (Success, Squeezed, Failed).
#[derive(Debug, Clone, PartialEq, Eq, Arbitrary)]
enum FinalTxStatus {
    /// The transaction was successfully included in a block.
    Success,
    /// The transaction was squeezed out of the txpool (or block)
    /// because it was not valid to include in the block.
    Squeezed,
    /// The transaction failed to execute and was included in a block.
    Failed,
}

/// Strategy to generate an Option<TransactionStatus>
fn state() -> impl Strategy<Value = Option<TransactionStatus>> {
    prop::option::of(transaction_status())
}

/// Strategy to generate a Result<Option<TransactionStatus>, Error>
fn state_result() -> impl Strategy<Value = Result<Option<TransactionStatus>, Error>> {
    prop::result::maybe_ok(state(), Just(Error))
}

/// Strategy to generate a TxStatusMessage
fn tx_status_message() -> impl Strategy<Value = TxStatusMessage> {
    prop_oneof![
        Just(TxStatusMessage::FailedStatus),
        transaction_status().prop_map(TxStatusMessage::Status),
    ]
}

/// Strategy to generate a TransactionStatus
fn transaction_status() -> impl Strategy<Value = TransactionStatus> {
    prop_oneof![
        Just(submitted()),
        Just(success()),
        Just(failed()),
        Just(squeezed()),
    ]
}

/// Strategy to generate a Vec<TxStatusMessage> of length 0 to 5
fn input_stream() -> impl Strategy<Value = Vec<TxStatusMessage>> {
    prop::collection::vec(tx_status_message(), 0..=5)
}

/// Represents all errors the transaction_status_change function can return.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Arbitrary)]
struct Error;

/// Struct representing a submitted transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Arbitrary)]
struct Submitted;

/// A model of the transaction status change functions control flow.
type Flow = ControlFlow<FinalTxStatus, Submitted>;

/// The `transaction_status_change_model` function is a simplified version of the real
/// `transaction_status_change` function. It takes an `Option` and an `Iterator` as input
/// and returns an `Iterator`. It models the behavior of the real function by simulating
/// how the status of a transaction changes when it's processed.
///
/// # Arguments
///
/// * `state` - An `Option` representing the initial state of the transaction, which can be either `None` or a `TxStatusMessage`.
/// * `stream` - An `Iterator` that represents the incoming stream of events (`TxStatusMessage`), simulating the real-time status changes of a transaction.
///
/// # Returns
///
/// This function returns an `Iterator` that yields a `Result` containing either a `TxStatus` or an `Error`.
/// `TxStatus` is a model of the real `TransactionStatus` type, which is used to represent the status of a transaction.
fn transaction_status_change_model(
    state: Option<TxStatusMessage>,
    stream: impl Iterator<Item = TxStatusMessage>,
) -> impl Iterator<Item = Result<TxStatus, Error>> {
    // Combine the initial state with the stream of incoming status messages.
    // Note that this option will turn into a empty iterator if the initial state is `None`.
    let out = state
        .into_iter()
        .chain(stream)
        .try_fold(Vec::new(), |mut out, state| match state {
            TxStatusMessage::Status(status) => match next_state(status) {
                // If the next state is "Continue" with "Submitted" status, push it to the output vector
                Flow::Continue(Submitted) => {
                    out.push(Ok(TxStatus::Submitted));
                    ControlFlow::Continue(out)
                }
                // If the next state is "Break" with a final status, push it to the output vector
                Flow::Break(r) => {
                    out.push(Ok(TxStatus::Final(r)));
                    ControlFlow::Break(out)
                }
            },
            // In case of a failed status, push the error to the output vector and break
            TxStatusMessage::FailedStatus => {
                out.push(Err(Error));
                ControlFlow::Break(out)
            }
        });

    // Convert the output into an iterator and return it
    match out {
        ControlFlow::Continue(out) | ControlFlow::Break(out) => out.into_iter(),
    }
}

/// This function models the behavior of the real function by determining the next transaction status.
/// Takes a `TransactionStatus` and returns a `Flow` value based on the given status.
/// If the status is `Submitted`, the function returns a `Flow::Continue` with `Submitted`.
/// If the status is `Success`, `SqueezedOut`, or `Failed`, the function returns a `Flow::Break` with the corresponding `FinalTxStatus`.
fn next_state(state: TransactionStatus) -> Flow {
    match state {
        TransactionStatus::Submitted { .. } => Flow::Continue(Submitted),
        TransactionStatus::Success { .. } => Flow::Break(FinalTxStatus::Success),
        TransactionStatus::Failed { .. } => Flow::Break(FinalTxStatus::Failed),
        TransactionStatus::SqueezedOut { .. } => Flow::Break(FinalTxStatus::Squeezed),
    }
}

// Thread-local to reuse a tokio Runtime
thread_local!(static RT: tokio::runtime::Runtime =
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
);

/// Property-based test for transaction_status_change
#[proptest]
fn test_tsc(
    #[strategy(state_result())] state: Result<Option<TransactionStatus>, Error>,
    #[strategy(input_stream())] stream: Vec<TxStatusMessage>,
) {
    test_tsc_inner(state, stream)
}

/// Helper function called by test_tsc to actually run the tests
fn test_tsc_inner(
    state: Result<Option<TransactionStatus>, Error>,
    stream: Vec<TxStatusMessage>,
) {
    let model_out: Vec<_> = transaction_status_change_model(
        state.clone().transpose().map(TxStatusMessage::from),
        stream.clone().into_iter(),
    )
    .collect();

    let out = RT.with(|rt| {
        rt.block_on(async {
            let mut mock_state = super::MockTxnStatusChangeState::new();
            mock_state
                .expect_get_tx_status()
                .returning(move |_| match state.clone() {
                    Ok(Some(t)) => Ok(Some(t)),
                    Ok(None) => Ok(None),
                    Err(_) => Err(StorageError::NotFound("", "")),
                });

            let stream = futures::stream::iter(stream).boxed();
            super::transaction_status_change(mock_state, stream, txn_id(0))
                .collect::<Vec<_>>()
                .await
        })
    });
    let out: Vec<_> = out
        .into_iter()
        .map(|r| r.map(|s| s.into()).map_err(|_| Error))
        .collect();
    assert_eq!(model_out, out);
}

impl From<crate::schema::tx::types::TransactionStatus> for TxStatus {
    fn from(status: crate::schema::tx::types::TransactionStatus) -> Self {
        match status {
            crate::schema::tx::types::TransactionStatus::Submitted(_) => {
                TxStatus::Submitted
            }
            crate::schema::tx::types::TransactionStatus::Success(_) => {
                TxStatus::Final(FinalTxStatus::Success)
            }
            crate::schema::tx::types::TransactionStatus::SqueezedOut(_) => {
                TxStatus::Final(FinalTxStatus::Squeezed)
            }
            crate::schema::tx::types::TransactionStatus::Failed(_) => {
                TxStatus::Final(FinalTxStatus::Failed)
            }
        }
    }
}
