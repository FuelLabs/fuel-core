use std::sync::Arc;

use super::{CommonChainConfig, Stores, TestChainSubstreams};

pub async fn chain(test_name: &str, stores: &Stores) -> TestChainSubstreams {
    let CommonChainConfig {
        logger_factory,
        mock_registry,
        chain_store,
        firehose_endpoints,
        ..
    } = CommonChainConfig::new(test_name, stores).await;

    let block_stream_builder = Arc::new(graph_chain_substreams::BlockStreamBuilder::new());

    let chain = Arc::new(graph_chain_substreams::Chain::new(
        logger_factory,
        firehose_endpoints,
        mock_registry,
        chain_store,
        block_stream_builder.clone(),
    ));

    TestChainSubstreams {
        chain,
        block_stream_builder,
    }
}
