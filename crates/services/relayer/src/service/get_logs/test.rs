#![allow(clippy::arithmetic_side_effects)]
use super::*;
use crate::{
    abi::bridge::{
        MessageSent,
        Transaction,
    },
    service::state::EthSyncGap,
    test_helpers::{
        page_sizer::IdentityPageSizer,
        provider::{
            MockProvider,
            TriggerType,
        },
    },
};
use alloy_primitives::{
    IntoLogData,
    U256,
};
use alloy_provider::transport::TransportError;
use std::{
    ops::RangeInclusive,
    sync::atomic::{
        self,
        AtomicUsize,
    },
};
use test_case::test_case;

fn messages_n(n: u64, nonce_offset: u64) -> Vec<Log> {
    messages(nonce_offset..=n + nonce_offset, 0..=n, 0..=0)
}

fn messages(
    nonce: RangeInclusive<u64>,
    block_number: RangeInclusive<u64>,
    contracts: RangeInclusive<u32>,
) -> Vec<Log> {
    let contracts = contracts.cycle();
    nonce
        .zip(block_number)
        .zip(contracts)
        .map(|((n, b), c)| message(n, b, c, 0))
        .collect()
}

fn message(nonce: u64, block_number: u64, contract_address: u32, log_index: u64) -> Log {
    let message = MessageSent {
        sender: Default::default(),
        recipient: Default::default(),
        nonce: U256::from(nonce),
        amount: 0,
        data: Default::default(),
    };
    let log_data = message.to_log_data();
    Log {
        inner: alloy_primitives::Log {
            address: u32_to_contract(contract_address),
            data: log_data.clone(),
        },
        block_hash: None,
        block_number: Some(block_number),
        block_timestamp: None,
        transaction_hash: None,
        transaction_index: None,
        log_index: Some(log_index),
        removed: false,
    }
}

fn transactions(
    nonce: RangeInclusive<u64>,
    block_number: RangeInclusive<u64>,
    contracts: RangeInclusive<u32>,
) -> Vec<Log> {
    let contracts = contracts.cycle();
    nonce
        .zip(block_number)
        .zip(contracts)
        .map(|((n, b), c)| transaction(n, b, c, 0))
        .collect()
}

fn transaction(nonce: u64, block_number: u64, contract_address: u32, index: u64) -> Log {
    let transaction = Transaction {
        nonce: U256::from(nonce),
        max_gas: Default::default(),
        canonically_serialized_tx: Default::default(),
    };
    let log_data = transaction.to_log_data();
    Log {
        inner: alloy_primitives::Log {
            address: u32_to_contract(contract_address),
            data: log_data.clone(),
        },
        block_hash: None,
        block_number: Some(block_number),
        block_timestamp: None,
        transaction_hash: None,
        transaction_index: None,
        log_index: Some(index),
        removed: false,
    }
}

fn contracts(c: &[u32]) -> Vec<alloy_primitives::Address> {
    c.iter().copied().map(u32_to_contract).collect()
}

fn u32_to_contract(n: u32) -> alloy_primitives::Address {
    let address: [u8; 20] = n
        .to_ne_bytes()
        .into_iter()
        .chain(core::iter::repeat(0u8))
        .take(20)
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    address.into()
}

#[derive(Clone, Debug)]
struct Input {
    eth_gap: RangeInclusive<u64>,
    c: Vec<alloy_primitives::Address>,
    m: Vec<Log>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Expected {
    num_get_logs_calls: usize,
    m: Vec<Log>,
}

const DEFAULT_LOG_PAGE_SIZE: u64 = 5;

#[test_case(
    Input {
        eth_gap: 0..=1,
        c: contracts(&[0]),
        m: messages(0..=0, 1..=1, 0..=0),
    }
    => Expected{ num_get_logs_calls: 1, m: messages(0..=0, 1..=1, 0..=0) }
    ; "Can get single log"
)]
#[test_case(
    Input {
        eth_gap: 0..=10,
        c: contracts(&[0]),
        m: messages(0..=10, 0..=10, 0..=0),
    }
    => Expected{ num_get_logs_calls: 3, m: messages(0..=10, 0..=10, 0..=0) }
    ; "Paginates for more than 5"
)]
#[test_case(
    Input {
        eth_gap: 4..=10,
        c: contracts(&[0]),
        m: messages(0..=10, 5..=16, 0..=0),
    }
    => Expected{ num_get_logs_calls: 2, m: messages(0..=10, 5..=10, 0..=0) }
    ; "Get messages from blocks 5..=10"
)]
#[test_case(
    Input {
        eth_gap: 0..=1,
        c: contracts(&[0]),
        m: {
            let mut logs = messages(0..=0, 1..=1, 0..=0);
            let txs = transactions(0..=0, 1..=1, 0..=0);
            logs.extend(txs);
            logs
        },
    }
    => Expected{
        num_get_logs_calls: 1,
        m: {
            let mut logs = messages(0..=0, 1..=1, 0..=0);
            let txs = transactions(0..=0, 1..=1, 0..=0);
            logs.extend(txs);
            logs
        }
    }
    ; "Filters MessageSent and Transaction events"
)]
#[tokio::test]
async fn can_paginate_logs(input: Input) -> Expected {
    let Input {
        eth_gap,
        c: contracts,
        m: logs,
    } = input;
    let eth_node = MockProvider::default();

    eth_node.update_data(|data| {
        data.logs_batch = vec![logs];
        data.best_block.header.number = *eth_gap.end();
    });
    let count = std::sync::Arc::new(AtomicUsize::new(0));
    let num_calls = count.clone();
    eth_node.set_after_event(move |_, evt| {
        if let TriggerType::GetLogs(_) = evt {
            count.fetch_add(1, atomic::Ordering::SeqCst);
        }
    });

    let result = download_logs(
        &EthSyncGap::new(*eth_gap.start(), *eth_gap.end()),
        contracts,
        &eth_node,
        &mut IdentityPageSizer::new(DEFAULT_LOG_PAGE_SIZE),
    )
    .map_ok(|logs| logs.logs)
    .try_concat()
    .await
    .unwrap();
    Expected {
        num_get_logs_calls: num_calls.load(atomic::Ordering::SeqCst),
        m: result,
    }
}

#[test_case(vec![
    Ok((1, 1, messages_n(1, 0)))
    ] => 1 ; "Can add single"
)]
#[test_case(vec![
    Ok((3, 3, messages_n(3, 0))),
    Ok((4, 4, messages_n(1, 4)))
    ] => 4 ; "Can add two"
)]
#[test_case(vec![
    Ok((3, 3, messages_n(3, 0))),
    Ok((4, 4, vec![]))
    ] => 4 ; "Can add empty"
)]
#[test_case(vec![
    Ok((1, 7, messages_n(3, 0))),
    Ok((8, 19, messages_n(1, 4))),
    Err(TransportError::NullResp)
    ] => 19 ; "Still adds height when error"
)]
#[tokio::test]
#[allow(clippy::type_complexity)]
async fn test_da_height_updates(
    stream: Vec<Result<(u64, u64, Vec<Log>), TransportError>>,
) -> u64 {
    let mut mock_db = crate::mock_db::MockDb::default();

    let logs = futures::stream::iter(stream).map(|result| {
        result.map(|(start_height, last_height, logs)| DownloadedLogs {
            start_height,
            last_height,
            logs,
        })
    });

    let _ = write_logs(&mut mock_db, logs).await;

    *mock_db.get_finalized_da_height().unwrap()
}
