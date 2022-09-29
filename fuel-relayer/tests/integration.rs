#![cfg(feature = "test-helpers")]

use std::{
    io::Read,
    sync::Arc,
};

use ethers_core::types::U256;
use fuel_core_interfaces::{
    common::{
        crypto,
        fuel_storage::StorageInspect,
        prelude::{
            Bytes32,
            Output,
            SizedBytes,
            Transaction,
        },
    },
    db::Messages,
    model::{
        FuelBlock,
        FuelBlockHeader,
        SealedFuelBlock,
    },
    relayer::RelayerDb,
};
use fuel_relayer::{
    bridge::message::SentMessageFilter,
    mock_db::MockDb,
    test_helpers::{
        middleware::{
            MockMiddleware,
            TriggerType,
        },
        EvtToLog,
        LogTestHelper,
    },
    Config,
    RelayerHandle,
    H256,
};

fn make_block(
    height: u32,
    eth_number: u64,
    prev_root: Bytes32,
    output_amount: u64,
) -> SealedFuelBlock {
    let mut out = Transaction::default();
    match &mut out {
        Transaction::Script { outputs, .. } | Transaction::Create { outputs, .. } => {
            *outputs = vec![Output::Message {
                recipient: Default::default(),
                amount: output_amount,
            }];
        }
    }
    SealedFuelBlock {
        block: FuelBlock {
            header: FuelBlockHeader {
                height: height.into(),
                da_height: eth_number.into(),
                prev_root,
                ..Default::default()
            },
            transactions: vec![out],
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

    relayer.await_synced().await.unwrap();

    assert_eq!(*mock_db.get_finalized_da_height().await, 100);
}

#[tokio::test]
async fn can_get_messages() {
    let mock_db = MockDb::default();
    let eth_node = MockMiddleware::default();

    let config = Config::default_test();
    let contract_address = config.eth_v2_listening_contracts[0].into();
    let message = |nonce, block_number: u64| {
        let message = SentMessageFilter {
            nonce,
            ..Default::default()
        };
        let mut log = message.into_log();
        log.address = contract_address;
        log.block_number = Some(block_number.into());
        log
    };

    let logs = vec![message(1, 3), message(2, 5)];
    let expected_messages: Vec<_> = logs.iter().map(|l| l.to_msg()).collect();
    eth_node.update_data(|data| data.logs_batch = vec![logs.clone()]);
    // Setup the eth node with a block high enough that there
    // will be some finalized blocks.
    eth_node.update_data(|data| data.best_block.number = Some(200.into()));
    let relayer = RelayerHandle::start_test(eth_node, Box::new(mock_db.clone()), config);

    relayer.await_synced().await.unwrap();

    for msg in expected_messages {
        assert_eq!(
            &*StorageInspect::<Messages>::get(&mock_db, msg.id())
                .unwrap()
                .unwrap(),
            &*msg
        );
    }
}

#[tokio::test]
async fn can_publish_fuel_block() {
    let mock_db = MockDb::default();
    let mut config = Config::default_test();
    config.da_finalization = 1u64.into();
    config.fuel_min_force_publish = 1;
    let eth_node = MockMiddleware::default();
    {
        let mut lock = mock_db.data.lock().unwrap();
        lock.chain_height = 1u32.into();
        lock.sealed_blocks.insert(
            1u32.into(),
            Arc::new(make_block(1, 1, Default::default(), 1)),
        );
        lock.sealed_blocks.insert(
            0u32.into(),
            Arc::new(make_block(0, 0, Default::default(), 1)),
        );
    };

    // Setup the eth node with a block high enough that there
    // will be some finalized blocks.
    eth_node.update_data(|data| data.best_block.number = Some(1.into()));

    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut tx = Some(tx);
    eth_node.set_after_event(move |data, event| {
        if let TriggerType::Send = event {
            assert_eq!(data.best_block.number.unwrap().as_u64(), 2);
            data.best_block.number = Some(data.best_block.number.unwrap() + 1);
            assert_eq!(data.incoming_message_roots.len(), 1);
            let expected_timestamp: [u8; 32] = U256::from(1).into();
            let expected_timestamp: H256 = expected_timestamp.into();
            let mut msg = Output::Message {
                recipient: Default::default(),
                amount: 1,
            };
            let mut buf = vec![0u8; msg.serialized_size()];
            msg.read(&mut buf).unwrap();
            let msg = crypto::ephemeral_merkle_root(vec![buf].into_iter());
            assert_eq!(
                *data.incoming_message_roots.get(&msg).unwrap(),
                expected_timestamp
            );
            tx.take().unwrap().send(()).unwrap();
        }
    });
    let relayer = RelayerHandle::start_test(eth_node, Box::new(mock_db.clone()), config);

    relayer.await_synced().await.unwrap();

    assert_eq!(
        mock_db.get_last_published_fuel_height().await,
        Some(1u32.into())
    );
    tokio::time::timeout(std::time::Duration::from_secs(5), rx)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn does_not_double_publish_fuel_block() {
    let mock_db = MockDb::default();
    let mut config = Config::default_test();
    config.da_finalization = 1u64.into();
    config.fuel_min_force_publish = 1;
    let eth_node = MockMiddleware::default();
    {
        let mut lock = mock_db.data.lock().unwrap();
        lock.chain_height = 1u32.into();
        lock.sealed_blocks.insert(
            1u32.into(),
            Arc::new(make_block(1, 1, Default::default(), 1)),
        );
        lock.sealed_blocks.insert(
            0u32.into(),
            Arc::new(make_block(0, 0, Default::default(), 1)),
        );
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
        TriggerType::Send => {
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
        mock_db.get_last_published_fuel_height().await,
        Some(1u32.into())
    );
}
