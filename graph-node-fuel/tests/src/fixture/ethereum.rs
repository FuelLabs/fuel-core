use std::marker::PhantomData;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::{
    test_ptr, CommonChainConfig, MutexBlockStreamBuilder, NoopAdapterSelector, NoopRuntimeAdapter,
    StaticBlockRefetcher, StaticStreamBuilder, Stores, TestChain,
};
use graph::blockchain::client::ChainClient;
use graph::blockchain::{BlockPtr, TriggersAdapterSelector};
use graph::cheap_clone::CheapClone;
use graph::prelude::ethabi::ethereum_types::H256;
use graph::prelude::web3::types::{Address, Log, Transaction, H160};
use graph::prelude::{ethabi, tiny_keccak, LightEthereumBlock, ENV_VARS};
use graph::{blockchain::block_stream::BlockWithTriggers, prelude::ethabi::ethereum_types::U64};
use graph_chain_ethereum::trigger::LogRef;
use graph_chain_ethereum::Chain;
use graph_chain_ethereum::{
    chain::BlockFinality,
    trigger::{EthereumBlockTriggerType, EthereumTrigger},
};

pub async fn chain(
    test_name: &str,
    blocks: Vec<BlockWithTriggers<Chain>>,
    stores: &Stores,
    triggers_adapter: Option<Arc<dyn TriggersAdapterSelector<Chain>>>,
) -> TestChain<Chain> {
    let triggers_adapter = triggers_adapter.unwrap_or(Arc::new(NoopAdapterSelector {
        triggers_in_block_sleep: Duration::ZERO,
        x: PhantomData,
    }));

    let CommonChainConfig {
        logger_factory,
        mock_registry,
        chain_store,
        firehose_endpoints,
        node_id,
    } = CommonChainConfig::new(test_name, stores).await;

    let client = Arc::new(ChainClient::<Chain>::new_firehose(firehose_endpoints));

    let static_block_stream = Arc::new(StaticStreamBuilder { chain: blocks });
    let block_stream_builder = Arc::new(MutexBlockStreamBuilder(Mutex::new(static_block_stream)));

    let chain = Chain::new(
        logger_factory,
        stores.network_name.clone(),
        node_id,
        mock_registry,
        chain_store.cheap_clone(),
        chain_store,
        client,
        stores.chain_head_listener.cheap_clone(),
        block_stream_builder.clone(),
        Arc::new(StaticBlockRefetcher { x: PhantomData }),
        triggers_adapter,
        Arc::new(NoopRuntimeAdapter { x: PhantomData }),
        ENV_VARS.reorg_threshold,
        ENV_VARS.ingestor_polling_interval,
        // We assume the tested chain is always ingestible for now
        true,
    );

    TestChain {
        chain: Arc::new(chain),
        block_stream_builder,
    }
}

pub fn genesis() -> BlockWithTriggers<graph_chain_ethereum::Chain> {
    let ptr = test_ptr(0);
    BlockWithTriggers::<graph_chain_ethereum::Chain> {
        block: BlockFinality::Final(Arc::new(LightEthereumBlock {
            hash: Some(H256::from_slice(ptr.hash.as_slice())),
            number: Some(U64::from(ptr.number)),
            ..Default::default()
        })),
        trigger_data: vec![EthereumTrigger::Block(ptr, EthereumBlockTriggerType::End)],
    }
}

pub fn generate_empty_blocks_for_range(
    parent_ptr: BlockPtr,
    start: i32,
    end: i32,
) -> Vec<BlockWithTriggers<graph_chain_ethereum::Chain>> {
    (start + 1..end + 1).fold(
        vec![empty_block(parent_ptr.clone(), test_ptr(start))],
        |mut blocks, i| {
            let parent_ptr = blocks.last().unwrap().ptr().clone();
            blocks.push(empty_block(parent_ptr, test_ptr(i)));
            blocks
        },
    )
}

pub fn empty_block(
    parent_ptr: BlockPtr,
    ptr: BlockPtr,
) -> BlockWithTriggers<graph_chain_ethereum::Chain> {
    assert!(ptr != parent_ptr);
    assert!(ptr.number > parent_ptr.number);

    // A 0x000.. transaction is used so `push_test_log` can use it
    let transactions = vec![Transaction {
        hash: H256::zero(),
        block_hash: Some(H256::from_slice(ptr.hash.as_slice())),
        block_number: Some(ptr.number.into()),
        transaction_index: Some(0.into()),
        from: Some(H160::zero()),
        to: Some(H160::zero()),
        ..Default::default()
    }];

    BlockWithTriggers::<graph_chain_ethereum::Chain> {
        block: BlockFinality::Final(Arc::new(LightEthereumBlock {
            hash: Some(H256::from_slice(ptr.hash.as_slice())),
            number: Some(U64::from(ptr.number)),
            parent_hash: H256::from_slice(parent_ptr.hash.as_slice()),
            transactions,
            ..Default::default()
        })),
        trigger_data: vec![EthereumTrigger::Block(ptr, EthereumBlockTriggerType::End)],
    }
}

pub fn push_test_log(block: &mut BlockWithTriggers<Chain>, payload: impl Into<String>) {
    let log = Arc::new(Log {
        address: Address::zero(),
        topics: vec![tiny_keccak::keccak256(b"TestEvent(string)").into()],
        data: ethabi::encode(&[ethabi::Token::String(payload.into())]).into(),
        block_hash: Some(H256::from_slice(block.ptr().hash.as_slice())),
        block_number: Some(block.ptr().number.into()),
        transaction_hash: Some(H256::from_low_u64_be(0)),
        transaction_index: Some(0.into()),
        log_index: Some(0.into()),
        transaction_log_index: Some(0.into()),
        log_type: None,
        removed: None,
    });
    block
        .trigger_data
        .push(EthereumTrigger::Log(LogRef::FullLog(log, None)))
}

pub fn push_test_polling_trigger(block: &mut BlockWithTriggers<Chain>) {
    block.trigger_data.push(EthereumTrigger::Block(
        block.ptr(),
        EthereumBlockTriggerType::End,
    ))
}
