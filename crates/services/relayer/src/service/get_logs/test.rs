#![allow(clippy::arithmetic_side_effects)]
use ethers_core::types::U256;
use std::{
    ops::RangeInclusive,
    sync::atomic::{
        self,
        AtomicUsize,
    },
};

use crate::{
    abi::bridge::MessageSentFilter,
    service::state::EthSyncGap,
    test_helpers::{
        middleware::{
            MockMiddleware,
            TriggerType,
        },
        EvtToLog,
    },
};
use test_case::test_case;

use super::*;

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
        .map(|((n, b), c)| message(n, b, c))
        .collect()
}

fn message(nonce: u64, block_number: u64, contract_address: u32) -> Log {
    let message = MessageSentFilter {
        nonce: U256::from_dec_str(nonce.to_string().as_str())
            .expect("Should convert into U256"),
        ..Default::default()
    };
    let mut log = message.into_log();
    log.address = u32_to_contract(contract_address);
    log.block_number = Some(block_number.into());
    log
}

fn contracts(c: &[u32]) -> Vec<H160> {
    c.iter().copied().map(u32_to_contract).collect()
}

fn u32_to_contract(n: u32) -> H160 {
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
    c: Vec<H160>,
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
#[tokio::test]
async fn can_paginate_logs(input: Input) -> Expected {
    let Input {
        eth_gap,
        c: contracts,
        m: logs,
    } = input;
    let eth_node = MockMiddleware::default();

    eth_node.update_data(|data| {
        data.logs_batch = vec![logs];
        data.best_block.number = Some((*eth_gap.end()).into());
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
        DEFAULT_LOG_PAGE_SIZE,
    )
    .map_ok(|(_, l)| l)
    .try_concat()
    .await
    .unwrap();
    Expected {
        num_get_logs_calls: num_calls.load(atomic::Ordering::SeqCst),
        m: result,
    }
}

#[test_case(vec![
    Ok((1, messages_n(1, 0)))
    ] => 1 ; "Can add single"
)]
#[test_case(vec![
    Ok((1, messages_n(3, 0))),
    Ok((2, messages_n(1, 4)))
    ] => 2 ; "Can add two"
)]
#[test_case(vec![
    Ok((1, messages_n(3, 0))),
    Ok((2, vec![]))
    ] => 2 ; "Can add empty"
)]
#[test_case(vec![
    Ok((7, messages_n(3, 0))),
    Ok((19, messages_n(1, 4))),
    Err(ProviderError::CustomError("".to_string()))
    ] => 19 ; "Still adds height when error"
)]
#[tokio::test]
async fn test_da_height_updates(
    stream: Vec<Result<(u64, Vec<Log>), ProviderError>>,
) -> u64 {
    let mut mock_db = crate::mock_db::MockDb::default();
    mock_db
        .set_finalized_da_height_to_at_least(&0u64.into())
        .unwrap();

    let logs = futures::stream::iter(stream);

    let _ = write_logs(&mut mock_db, logs).await;

    *mock_db.get_finalized_da_height().unwrap()
}
