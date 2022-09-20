#![cfg(feature = "test-helpers")]

use std::sync::Arc;

use ethers_contract::EthEvent;
use ethers_core::{
    abi::Tokenizable,
    types::Log,
};
use fuel_core_interfaces::{
    common::prelude::{
        Bytes32,
        Storage,
    },
    model::{
        FuelBlock,
        FuelBlockHeader,
        SealedFuelBlock,
    },
    relayer::RelayerDb,
};
use fuel_relayer_new::{
    bridge::message::SentMessageFilter,
    fuel::fuel::BlockCommittedFilter,
    mock_db::MockDb,
    test_helpers::{
        middleware::{
            MockMiddleware,
            TriggerType,
        },
        LogTestHelper,
    },
    Config,
    RelayerHandle,
    H256,
};

fn make_block(height: u32, eth_number: u64, prev_root: Bytes32) -> SealedFuelBlock {
    SealedFuelBlock {
        block: FuelBlock {
            header: FuelBlockHeader {
                height: height.into(),
                number: eth_number.into(),
                prev_root,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

#[tokio::test]
async fn can_set_da_height() {
    let mock_db = MockDb::default();
    let eth_node = MockMiddleware::default();
    // Setup the eth node with a block high enough that there
    // will be some finalized blocks.
    eth_node.update_data(|data| data.best_block.number = Some(200.into()));
    let relayer = RelayerHandle::start_test(
        eth_node,
        Box::new(mock_db.clone()),
        Default::default(),
    );

    dbg!();
    relayer.await_synced().await.unwrap();
    dbg!();

    assert_eq!(mock_db.get_finalized_da_height().await, 100);
}

#[tokio::test]
async fn can_get_messages() {
    let mock_db = MockDb::default();
    let eth_node = MockMiddleware::default();

    let topics = vec![
        SentMessageFilter::signature(),
        H256::default(),
        H256::default(),
    ];
    let message = |nonce| {
        let message = SentMessageFilter {
            nonce,
            ..Default::default()
        };
        let mut message = message.into_token();
        match &mut message {
            ethers_core::abi::Token::Tuple(message) => {
                message.remove(0);
                message.remove(0);
            }
            _ => (),
        }
        ethers_core::abi::encode(&[message])
    };

    let config = Config::default_test();

    let logs = vec![
        Log {
            address: config.eth_v2_listening_contracts[0].into(),
            topics: topics.clone(),
            data: message(1).into(),
            block_number: Some(3.into()),
            ..Default::default()
        },
        Log {
            address: config.eth_v2_listening_contracts[0].into(),
            topics,
            data: message(2).into(),
            block_number: Some(5.into()),
            ..Default::default()
        },
    ];
    let expected_messages: Vec<_> = logs.iter().map(|l| l.to_msg()).collect();
    eth_node.update_data(|data| data.logs_batch = vec![logs.clone()]);
    // Setup the eth node with a block high enough that there
    // will be some finalized blocks.
    eth_node.update_data(|data| data.best_block.number = Some(200.into()));
    let relayer = RelayerHandle::start_test(eth_node, Box::new(mock_db.clone()), config);

    relayer.await_synced().await.unwrap();

    for msg in expected_messages {
        assert_eq!(mock_db.get(msg.id()).unwrap().unwrap().as_ref(), &*msg);
    }
}

#[tokio::test]
async fn can_get_committed_block() {
    let mock_db = MockDb::default();
    let eth_node = MockMiddleware::default();

    let topics = |height: u32| {
        let mut h = vec![0u8; 28];
        h.extend(height.to_be_bytes());
        let h: [u8; 32] = h.try_into().unwrap();
        vec![BlockCommittedFilter::signature(), H256::default(), h.into()]
    };

    let config = Config::default_test();

    let logs = vec![
        Log {
            address: config.eth_v2_listening_contracts[0].into(),
            topics: topics(3),
            data: vec![].into(),
            block_number: Some(3.into()),
            ..Default::default()
        },
        Log {
            address: config.eth_v2_listening_contracts[0].into(),
            topics: topics(5),
            data: vec![].into(),
            block_number: Some(5.into()),
            ..Default::default()
        },
    ];
    eth_node.update_data(|data| data.logs_batch = vec![logs.clone()]);
    // Setup the eth node with a block high enough that there
    // will be some finalized blocks.
    eth_node.update_data(|data| data.best_block.number = Some(200.into()));
    let relayer = RelayerHandle::start_test(eth_node, Box::new(mock_db.clone()), config);

    relayer.await_synced().await.unwrap();

    assert_eq!(
        mock_db.get_last_committed_finalized_fuel_height().await,
        5u64.into()
    );
}

#[tokio::test]
async fn can_publish_fuel_block() {
    let mock_db = MockDb::default();
    let mut config = Config::default_test();
    config.da_finalization = 1u64.into();
    let eth_node = MockMiddleware::default();
    {
        let mut lock = mock_db.data.lock().unwrap();
        lock.chain_height = 1u32.into();
        lock.sealed_blocks
            .insert(1u32.into(), Arc::new(make_block(1, 1, Default::default())));
    };
    // Setup the eth node with a block high enough that there
    // will be some finalized blocks.
    eth_node.update_data(|data| data.best_block.number = Some(1.into()));
    eth_node.set_after_event(|data, event| {
        if let TriggerType::Call = event {
            assert_eq!(data.best_block.number.unwrap().as_u64(), 2);
            data.best_block.number = Some(data.best_block.number.unwrap() + 1);
        }
    });
    let relayer = RelayerHandle::start_test(eth_node, Box::new(mock_db.clone()), config);

    relayer.await_synced().await.unwrap();

    assert_eq!(
        mock_db.get_last_committed_finalized_fuel_height().await,
        1u32.into()
    );
}

#[tokio::test]
async fn does_not_double_publish_fuel_block() {
    let mock_db = MockDb::default();
    let mut config = Config::default_test();
    config.da_finalization = 1u64.into();
    let eth_node = MockMiddleware::default();
    {
        let mut lock = mock_db.data.lock().unwrap();
        lock.chain_height = 1u32.into();
        lock.sealed_blocks
            .insert(1u32.into(), Arc::new(make_block(1, 1, Default::default())));
    };
    // Setup the eth node with a block high enough that there
    // will be some finalized blocks.
    eth_node.update_data(|data| data.best_block.number = Some(1.into()));

    let mut counter = 0;
    eth_node.set_state_override(move |data| {
        if counter == 0 && !data.logs_batch.is_empty() {
            data.logs_batch.clear();
            counter += 1;
        }
    });

    let mut call_counter = 0;
    let mut get_block_counter = 0;
    let db = mock_db.clone();
    eth_node.set_after_event(move |data, event| match event {
        TriggerType::Call => {
            call_counter += 1;
            if get_block_counter < 10 {
                assert!(call_counter == 1);
            }
            data.best_block.number = Some(data.best_block.number.unwrap() + 1);
        }
        TriggerType::GetBlockNumber => {
            get_block_counter += 1;
            if get_block_counter == 10 {
                db.data.lock().unwrap().pending_committed_fuel_height = None;
            }
        }
        _ => (),
    });

    let relayer = RelayerHandle::start_test(eth_node, Box::new(mock_db.clone()), config);

    relayer.await_synced().await.unwrap();

    assert_eq!(
        mock_db.get_last_committed_finalized_fuel_height().await,
        1u32.into()
    );
}
