use std::sync::Arc;

use graph::{
    blockchain::{block_stream::BlockWithTriggers, BlockPtr},
    prelude::{
        web3::types::{Address, Bytes, Log, H160, H256, U64},
        EthereumCall, LightEthereumBlock,
    },
    slog::{self, o, Logger},
};

use crate::{
    chain::BlockFinality,
    trigger::{EthereumBlockTriggerType, EthereumTrigger, LogRef},
};

#[test]
fn test_trigger_ordering() {
    let block1 = EthereumTrigger::Block(
        BlockPtr::from((H256::random(), 1u64)),
        EthereumBlockTriggerType::End,
    );

    let block2 = EthereumTrigger::Block(
        BlockPtr::from((H256::random(), 0u64)),
        EthereumBlockTriggerType::WithCallTo(Address::random()),
    );

    let mut call1 = EthereumCall::default();
    call1.transaction_index = 1;
    let call1 = EthereumTrigger::Call(Arc::new(call1));

    let mut call2 = EthereumCall::default();
    call2.transaction_index = 2;
    call2.input = Bytes(vec![0]);
    let call2 = EthereumTrigger::Call(Arc::new(call2));

    let mut call3 = EthereumCall::default();
    call3.transaction_index = 3;
    let call3 = EthereumTrigger::Call(Arc::new(call3));

    // Call with the same tx index as call2
    let mut call4 = EthereumCall::default();
    call4.transaction_index = 2;
    // different than call2 so they don't get mistaken as the same
    call4.input = Bytes(vec![1]);
    let call4 = EthereumTrigger::Call(Arc::new(call4));

    fn create_log(tx_index: u64, log_index: u64) -> Arc<Log> {
        Arc::new(Log {
            address: H160::default(),
            topics: vec![],
            data: Bytes::default(),
            block_hash: Some(H256::zero()),
            block_number: Some(U64::zero()),
            transaction_hash: Some(H256::zero()),
            transaction_index: Some(tx_index.into()),
            log_index: Some(log_index.into()),
            transaction_log_index: Some(log_index.into()),
            log_type: Some("".into()),
            removed: Some(false),
        })
    }

    // Event with transaction_index 1 and log_index 0;
    // should be the first element after sorting
    let log1 = EthereumTrigger::Log(LogRef::FullLog(create_log(1, 0), None));

    // Event with transaction_index 1 and log_index 1;
    // should be the second element after sorting
    let log2 = EthereumTrigger::Log(LogRef::FullLog(create_log(1, 1), None));

    // Event with transaction_index 2 and log_index 5;
    // should come after call1 and before call2 after sorting
    let log3 = EthereumTrigger::Log(LogRef::FullLog(create_log(2, 5), None));

    let triggers = vec![
        // Call triggers; these should be in the order 1, 2, 4, 3 after sorting
        call3.clone(),
        call1.clone(),
        call2.clone(),
        call4.clone(),
        // Block triggers; these should appear at the end after sorting
        // but with their order unchanged
        block2.clone(),
        block1.clone(),
        // Event triggers
        log3.clone(),
        log2.clone(),
        log1.clone(),
    ];

    let logger = Logger::root(slog::Discard, o!());

    let mut b: LightEthereumBlock = Default::default();

    // This is necessary because inside of BlockWithTriggers::new
    // there's a log for both fields. So just using Default above
    // gives None on them.
    b.number = Some(Default::default());
    b.hash = Some(Default::default());

    // Test that `BlockWithTriggers` sorts the triggers.
    let block_with_triggers = BlockWithTriggers::<crate::Chain>::new(
        BlockFinality::Final(Arc::new(b)),
        triggers,
        &logger,
    );

    assert_eq!(
        block_with_triggers.trigger_data,
        vec![log1, log2, call1, log3, call2, call4, call3, block2, block1]
    );
}

#[test]
fn test_trigger_dedup() {
    let block1 = EthereumTrigger::Block(
        BlockPtr::from((H256::random(), 1u64)),
        EthereumBlockTriggerType::End,
    );

    let block2 = EthereumTrigger::Block(
        BlockPtr::from((H256::random(), 0u64)),
        EthereumBlockTriggerType::WithCallTo(Address::random()),
    );

    // duplicate block2
    let block3 = block2.clone();

    let mut call1 = EthereumCall::default();
    call1.transaction_index = 1;
    let call1 = EthereumTrigger::Call(Arc::new(call1));

    let mut call2 = EthereumCall::default();
    call2.transaction_index = 2;
    let call2 = EthereumTrigger::Call(Arc::new(call2));

    let mut call3 = EthereumCall::default();
    call3.transaction_index = 3;
    let call3 = EthereumTrigger::Call(Arc::new(call3));

    // duplicate call2
    let mut call4 = EthereumCall::default();
    call4.transaction_index = 2;
    let call4 = EthereumTrigger::Call(Arc::new(call4));

    fn create_log(tx_index: u64, log_index: u64) -> Arc<Log> {
        Arc::new(Log {
            address: H160::default(),
            topics: vec![],
            data: Bytes::default(),
            block_hash: Some(H256::zero()),
            block_number: Some(U64::zero()),
            transaction_hash: Some(H256::zero()),
            transaction_index: Some(tx_index.into()),
            log_index: Some(log_index.into()),
            transaction_log_index: Some(log_index.into()),
            log_type: Some("".into()),
            removed: Some(false),
        })
    }

    let log1 = EthereumTrigger::Log(LogRef::FullLog(create_log(1, 0), None));
    let log2 = EthereumTrigger::Log(LogRef::FullLog(create_log(1, 1), None));
    let log3 = EthereumTrigger::Log(LogRef::FullLog(create_log(2, 5), None));
    // duplicate logs 2 and 3
    let log4 = log2.clone();
    let log5 = log3.clone();

    let triggers = vec![
        // Call triggers
        call3.clone(),
        call1.clone(),
        call2.clone(),
        call4,
        // Block triggers
        block3,
        block2.clone(),
        block1.clone(),
        // Event triggers
        log5,
        log4,
        log3.clone(),
        log2.clone(),
        log1.clone(),
    ];

    let logger = Logger::root(slog::Discard, o!());

    let mut b: LightEthereumBlock = Default::default();

    // This is necessary because inside of BlockWithTriggers::new
    // there's a log for both fields. So just using Default above
    // gives None on them.
    b.number = Some(Default::default());
    b.hash = Some(Default::default());

    // Test that `BlockWithTriggers` sorts the triggers.
    let block_with_triggers = BlockWithTriggers::<crate::Chain>::new(
        BlockFinality::Final(Arc::new(b)),
        triggers,
        &logger,
    );

    assert_eq!(
        block_with_triggers.trigger_data,
        vec![log1, log2, call1, log3, call2, call3, block2, block1]
    );
}
