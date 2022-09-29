use super::*;

#[tokio::test]
async fn can_paginate_logs() {
    // let eth_node = MockMiddleware::default();

    // let config = Config::default_test();
    // let contract_address = config.eth_v2_listening_contracts[0].into();
    // let message = |nonce, block_number: u64| {
    //     let message = SentMessageFilter {
    //         nonce,
    //         ..Default::default()
    //     };
    //     let mut log = message.into_log();
    //     log.address = contract_address;
    //     log.block_number = Some(block_number.into());
    //     log
    // };

    // let logs = vec![message(1, 3), message(2, 5)];
    // let expected_messages: Vec<_> = logs.iter().map(|l| l.to_msg()).collect();
    // eth_node.update_data(|data| data.logs_batch = vec![logs.clone()]);
    // // Setup the eth node with a block high enough that there
    // // will be some finalized blocks.
    // eth_node.update_data(|data| data.best_block.number = Some(200.into()));
    // let relayer = RelayerHandle::start_test(eth_node, Box::new(mock_db.clone()), config);

    // relayer.await_synced().await.unwrap();

    // for msg in expected_messages {
    //     assert_eq!(
    //         &*StorageInspect::<Messages>::get(&mock_db, msg.id())
    //             .unwrap()
    //             .unwrap(),
    //         &*msg
    //     );
    // }
    
}