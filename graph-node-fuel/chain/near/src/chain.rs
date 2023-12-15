use anyhow::anyhow;
use graph::blockchain::client::ChainClient;
use graph::blockchain::firehose_block_ingestor::FirehoseBlockIngestor;
use graph::blockchain::substreams_block_stream::SubstreamsBlockStream;
use graph::blockchain::{
    BasicBlockchainBuilder, BlockIngestor, BlockchainBuilder, BlockchainKind, NoopRuntimeAdapter,
};
use graph::cheap_clone::CheapClone;
use graph::components::store::DeploymentCursorTracker;
use graph::data::subgraph::UnifiedMappingApiVersion;
use graph::env::EnvVars;
use graph::firehose::FirehoseEndpoint;
use graph::prelude::{MetricsRegistry, TryFutureExt};
use graph::schema::InputSchema;
use graph::substreams::{Clock, Package};
use graph::{
    anyhow::Result,
    blockchain::{
        block_stream::{
            BlockStreamEvent, BlockWithTriggers, FirehoseError,
            FirehoseMapper as FirehoseMapperTrait, TriggersAdapter as TriggersAdapterTrait,
        },
        firehose_block_stream::FirehoseBlockStream,
        BlockHash, BlockPtr, Blockchain, EmptyNodeCapabilities, IngestorError,
        RuntimeAdapter as RuntimeAdapterTrait,
    },
    components::store::DeploymentLocator,
    firehose::{self as firehose, ForkStep},
    prelude::{async_trait, o, BlockNumber, ChainStore, Error, Logger, LoggerFactory},
};
use prost::Message;
use std::sync::Arc;

use crate::adapter::TriggerFilter;
use crate::codec::substreams_triggers::BlockAndReceipts;
use crate::data_source::{DataSourceTemplate, UnresolvedDataSourceTemplate};
use crate::trigger::{self, NearTrigger};
use crate::{
    codec,
    data_source::{DataSource, UnresolvedDataSource},
};
use graph::blockchain::block_stream::{
    BlockStream, BlockStreamBuilder, BlockStreamMapper, FirehoseCursor,
};

const NEAR_FILTER_MODULE_NAME: &str = "near_filter";
const SUBSTREAMS_TRIGGER_FILTER_BYTES: &[u8; 510162] = include_bytes!(
    "../../../substreams/substreams-trigger-filter/substreams-trigger-filter-v0.1.0.spkg"
);

pub struct NearStreamBuilder {}

#[async_trait]
impl BlockStreamBuilder<Chain> for NearStreamBuilder {
    async fn build_substreams(
        &self,
        chain: &Chain,
        _schema: InputSchema,
        deployment: DeploymentLocator,
        block_cursor: FirehoseCursor,
        subgraph_current_block: Option<BlockPtr>,
        filter: Arc<<Chain as Blockchain>::TriggerFilter>,
    ) -> Result<Box<dyn BlockStream<Chain>>> {
        let mapper = Arc::new(FirehoseMapper {
            adapter: Arc::new(TriggersAdapter {}),
            filter,
        });
        let mut package =
            Package::decode(SUBSTREAMS_TRIGGER_FILTER_BYTES.to_vec().as_ref()).unwrap();
        match package.modules.as_mut() {
            Some(modules) => modules
                .modules
                .iter_mut()
                .find(|module| module.name == NEAR_FILTER_MODULE_NAME)
                .map(|module| {
                    graph::substreams::patch_module_params(
                        mapper.filter.to_module_params(),
                        module,
                    );
                    module
                }),
            None => None,
        };

        let logger = chain
            .logger_factory
            .subgraph_logger(&deployment)
            .new(o!("component" => "SubstreamsBlockStream"));
        let start_block = subgraph_current_block
            .as_ref()
            .map(|b| b.number)
            .unwrap_or_default();

        Ok(Box::new(SubstreamsBlockStream::new(
            deployment.hash,
            chain.chain_client(),
            subgraph_current_block,
            block_cursor.as_ref().clone(),
            mapper,
            package.modules.clone(),
            NEAR_FILTER_MODULE_NAME.to_string(),
            vec![start_block],
            vec![],
            logger,
            chain.metrics_registry.clone(),
        )))
    }

    async fn build_firehose(
        &self,
        chain: &Chain,
        deployment: DeploymentLocator,
        block_cursor: FirehoseCursor,
        start_blocks: Vec<BlockNumber>,
        subgraph_current_block: Option<BlockPtr>,
        filter: Arc<<Chain as Blockchain>::TriggerFilter>,
        unified_api_version: UnifiedMappingApiVersion,
    ) -> Result<Box<dyn BlockStream<Chain>>> {
        let adapter = chain
            .triggers_adapter(
                &deployment,
                &EmptyNodeCapabilities::default(),
                unified_api_version,
            )
            .unwrap_or_else(|_| panic!("no adapter for network {}", chain.name));

        let logger = chain
            .logger_factory
            .subgraph_logger(&deployment)
            .new(o!("component" => "FirehoseBlockStream"));

        let firehose_mapper = Arc::new(FirehoseMapper { adapter, filter });

        Ok(Box::new(FirehoseBlockStream::new(
            deployment.hash,
            chain.chain_client(),
            subgraph_current_block,
            block_cursor,
            firehose_mapper,
            start_blocks,
            logger,
            chain.metrics_registry.clone(),
        )))
    }

    async fn build_polling(
        &self,
        _chain: &Chain,
        _deployment: DeploymentLocator,
        _start_blocks: Vec<BlockNumber>,
        _subgraph_current_block: Option<BlockPtr>,
        _filter: Arc<<Chain as Blockchain>::TriggerFilter>,
        _unified_api_version: UnifiedMappingApiVersion,
    ) -> Result<Box<dyn BlockStream<Chain>>> {
        todo!()
    }
}

pub struct Chain {
    logger_factory: LoggerFactory,
    name: String,
    client: Arc<ChainClient<Self>>,
    chain_store: Arc<dyn ChainStore>,
    metrics_registry: Arc<MetricsRegistry>,
    block_stream_builder: Arc<dyn BlockStreamBuilder<Self>>,
    prefer_substreams: bool,
}

impl std::fmt::Debug for Chain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "chain: near")
    }
}

impl BlockchainBuilder<Chain> for BasicBlockchainBuilder {
    fn build(self, config: &Arc<EnvVars>) -> Chain {
        Chain {
            logger_factory: self.logger_factory,
            name: self.name,
            chain_store: self.chain_store,
            client: Arc::new(ChainClient::new_firehose(self.firehose_endpoints)),
            metrics_registry: self.metrics_registry,
            block_stream_builder: Arc::new(NearStreamBuilder {}),
            prefer_substreams: config.prefer_substreams_block_streams,
        }
    }
}

#[async_trait]
impl Blockchain for Chain {
    const KIND: BlockchainKind = BlockchainKind::Near;

    type Client = ();
    type Block = codec::Block;

    type DataSource = DataSource;

    type UnresolvedDataSource = UnresolvedDataSource;

    type DataSourceTemplate = DataSourceTemplate;

    type UnresolvedDataSourceTemplate = UnresolvedDataSourceTemplate;

    type TriggerData = crate::trigger::NearTrigger;

    type MappingTrigger = crate::trigger::NearTrigger;

    type TriggerFilter = crate::adapter::TriggerFilter;

    type NodeCapabilities = EmptyNodeCapabilities<Chain>;

    fn triggers_adapter(
        &self,
        _loc: &DeploymentLocator,
        _capabilities: &Self::NodeCapabilities,
        _unified_api_version: UnifiedMappingApiVersion,
    ) -> Result<Arc<dyn TriggersAdapterTrait<Self>>, Error> {
        let adapter = TriggersAdapter {};
        Ok(Arc::new(adapter))
    }

    async fn new_block_stream(
        &self,
        deployment: DeploymentLocator,
        store: impl DeploymentCursorTracker,
        start_blocks: Vec<BlockNumber>,
        filter: Arc<Self::TriggerFilter>,
        unified_api_version: UnifiedMappingApiVersion,
    ) -> Result<Box<dyn BlockStream<Self>>, Error> {
        if self.prefer_substreams {
            return self
                .block_stream_builder
                .build_substreams(
                    self,
                    store.input_schema(),
                    deployment,
                    store.firehose_cursor(),
                    store.block_ptr(),
                    filter,
                )
                .await;
        }

        self.block_stream_builder
            .build_firehose(
                self,
                deployment,
                store.firehose_cursor(),
                start_blocks,
                store.block_ptr(),
                filter,
                unified_api_version,
            )
            .await
    }

    fn is_refetch_block_required(&self) -> bool {
        false
    }

    async fn refetch_firehose_block(
        &self,
        _logger: &Logger,
        _cursor: FirehoseCursor,
    ) -> Result<codec::Block, Error> {
        unimplemented!("This chain does not support Dynamic Data Sources. is_refetch_block_required always returns false, this shouldn't be called.")
    }

    fn chain_store(&self) -> Arc<dyn ChainStore> {
        self.chain_store.clone()
    }

    async fn block_pointer_from_number(
        &self,
        logger: &Logger,
        number: BlockNumber,
    ) -> Result<BlockPtr, IngestorError> {
        let firehose_endpoint = self.client.firehose_endpoint()?;

        firehose_endpoint
            .block_ptr_for_number::<codec::HeaderOnlyBlock>(logger, number)
            .map_err(Into::into)
            .await
    }

    fn runtime_adapter(&self) -> Arc<dyn RuntimeAdapterTrait<Self>> {
        Arc::new(NoopRuntimeAdapter::default())
    }

    fn chain_client(&self) -> Arc<ChainClient<Self>> {
        self.client.clone()
    }

    fn block_ingestor(&self) -> anyhow::Result<Box<dyn BlockIngestor>> {
        let ingestor = FirehoseBlockIngestor::<crate::HeaderOnlyBlock, Self>::new(
            self.chain_store.cheap_clone(),
            self.chain_client(),
            self.logger_factory
                .component_logger("NearFirehoseBlockIngestor", None),
            self.name.clone(),
        );
        Ok(Box::new(ingestor))
    }
}

pub struct TriggersAdapter {}

#[async_trait]
impl TriggersAdapterTrait<Chain> for TriggersAdapter {
    async fn scan_triggers(
        &self,
        _from: BlockNumber,
        _to: BlockNumber,
        _filter: &TriggerFilter,
    ) -> Result<Vec<BlockWithTriggers<Chain>>, Error> {
        panic!("Should never be called since not used by FirehoseBlockStream")
    }

    async fn triggers_in_block(
        &self,
        logger: &Logger,
        block: codec::Block,
        filter: &TriggerFilter,
    ) -> Result<BlockWithTriggers<Chain>, Error> {
        // TODO: Find the best place to introduce an `Arc` and avoid this clone.
        let shared_block = Arc::new(block.clone());

        let TriggerFilter {
            block_filter,
            receipt_filter,
        } = filter;

        // Filter non-successful or non-action receipts.
        let receipts = block.shards.iter().flat_map(|shard| {
            shard
                .receipt_execution_outcomes
                .iter()
                .filter_map(|outcome| {
                    if !outcome
                        .execution_outcome
                        .as_ref()?
                        .outcome
                        .as_ref()?
                        .status
                        .as_ref()?
                        .is_success()
                    {
                        return None;
                    }
                    if !matches!(
                        outcome.receipt.as_ref()?.receipt,
                        Some(codec::receipt::Receipt::Action(_))
                    ) {
                        return None;
                    }

                    let receipt = outcome.receipt.as_ref()?.clone();
                    if !receipt_filter.matches(&receipt.receiver_id) {
                        return None;
                    }

                    Some(trigger::ReceiptWithOutcome {
                        outcome: outcome.execution_outcome.as_ref()?.clone(),
                        receipt,
                        block: shared_block.cheap_clone(),
                    })
                })
        });

        let mut trigger_data: Vec<_> = receipts
            .map(|r| NearTrigger::Receipt(Arc::new(r)))
            .collect();

        if block_filter.trigger_every_block {
            trigger_data.push(NearTrigger::Block(shared_block.cheap_clone()));
        }

        Ok(BlockWithTriggers::new(block, trigger_data, logger))
    }

    async fn is_on_main_chain(&self, _ptr: BlockPtr) -> Result<bool, Error> {
        panic!("Should never be called since not used by FirehoseBlockStream")
    }

    async fn ancestor_block(
        &self,
        _ptr: BlockPtr,
        _offset: BlockNumber,
    ) -> Result<Option<codec::Block>, Error> {
        panic!("Should never be called since FirehoseBlockStream cannot resolve it")
    }

    /// Panics if `block` is genesis.
    /// But that's ok since this is only called when reverting `block`.
    async fn parent_ptr(&self, block: &BlockPtr) -> Result<Option<BlockPtr>, Error> {
        // FIXME (NEAR):  Might not be necessary for NEAR support for now
        Ok(Some(BlockPtr {
            hash: BlockHash::from(vec![0xff; 32]),
            number: block.number.saturating_sub(1),
        }))
    }
}

pub struct FirehoseMapper {
    adapter: Arc<dyn TriggersAdapterTrait<Chain>>,
    filter: Arc<TriggerFilter>,
}

#[async_trait]
impl BlockStreamMapper<Chain> for FirehoseMapper {
    fn decode_block(&self, output: Option<&[u8]>) -> Result<Option<codec::Block>, Error> {
        let block = match output {
            Some(block) => codec::Block::decode(block)?,
            None => anyhow::bail!("near mapper is expected to always have a block"),
        };

        Ok(Some(block))
    }

    async fn block_with_triggers(
        &self,
        logger: &Logger,
        block: codec::Block,
    ) -> Result<BlockWithTriggers<Chain>, Error> {
        self.adapter
            .triggers_in_block(logger, block, self.filter.as_ref())
            .await
    }

    async fn handle_substreams_block(
        &self,
        _logger: &Logger,
        _clock: Clock,
        cursor: FirehoseCursor,
        message: Vec<u8>,
    ) -> Result<BlockStreamEvent<Chain>, Error> {
        let BlockAndReceipts {
            block,
            outcome,
            receipt,
        } = BlockAndReceipts::decode(message.as_ref())?;
        let block = block.ok_or_else(|| anyhow!("near block is mandatory on substreams"))?;
        let arc_block = Arc::new(block.clone());

        let trigger_data = outcome
            .into_iter()
            .zip(receipt.into_iter())
            .map(|(outcome, receipt)| {
                NearTrigger::Receipt(Arc::new(trigger::ReceiptWithOutcome {
                    outcome,
                    receipt,
                    block: arc_block.clone(),
                }))
            })
            .collect();

        Ok(BlockStreamEvent::ProcessBlock(
            BlockWithTriggers {
                block,
                trigger_data,
            },
            cursor,
        ))
    }
}

#[async_trait]
impl FirehoseMapperTrait<Chain> for FirehoseMapper {
    fn trigger_filter(&self) -> &TriggerFilter {
        self.filter.as_ref()
    }

    async fn to_block_stream_event(
        &self,
        logger: &Logger,
        response: &firehose::Response,
    ) -> Result<BlockStreamEvent<Chain>, FirehoseError> {
        let step = ForkStep::from_i32(response.step).unwrap_or_else(|| {
            panic!(
                "unknown step i32 value {}, maybe you forgot update & re-regenerate the protobuf definitions?",
                response.step
            )
        });

        let any_block = response
            .block
            .as_ref()
            .expect("block payload information should always be present");

        // Right now, this is done in all cases but in reality, with how the BlockStreamEvent::Revert
        // is defined right now, only block hash and block number is necessary. However, this information
        // is not part of the actual bstream::BlockResponseV2 payload. As such, we need to decode the full
        // block which is useless.
        //
        // Check about adding basic information about the block in the bstream::BlockResponseV2 or maybe
        // define a slimmed down stuct that would decode only a few fields and ignore all the rest.
        // unwrap: Input cannot be None so output will be error or block.
        let block = self.decode_block(Some(any_block.value.as_ref()))?.unwrap();

        use ForkStep::*;
        match step {
            StepNew => Ok(BlockStreamEvent::ProcessBlock(
                self.block_with_triggers(logger, block).await?,
                FirehoseCursor::from(response.cursor.clone()),
            )),

            StepUndo => {
                let parent_ptr = block
                    .header()
                    .parent_ptr()
                    .expect("Genesis block should never be reverted");

                Ok(BlockStreamEvent::Revert(
                    parent_ptr,
                    FirehoseCursor::from(response.cursor.clone()),
                ))
            }

            StepFinal => {
                panic!("irreversible step is not handled and should not be requested in the Firehose request")
            }

            StepUnset => {
                panic!("unknown step should not happen in the Firehose response")
            }
        }
    }

    async fn block_ptr_for_number(
        &self,
        logger: &Logger,
        endpoint: &Arc<FirehoseEndpoint>,
        number: BlockNumber,
    ) -> Result<BlockPtr, Error> {
        endpoint
            .block_ptr_for_number::<codec::HeaderOnlyBlock>(logger, number)
            .await
    }

    async fn final_block_ptr_for(
        &self,
        logger: &Logger,
        endpoint: &Arc<FirehoseEndpoint>,
        block: &codec::Block,
    ) -> Result<BlockPtr, Error> {
        let final_block_number = block.header().last_final_block_height as BlockNumber;

        self.block_ptr_for_number(logger, endpoint, final_block_number)
            .await
    }
}

#[cfg(test)]
mod test {
    use std::{collections::HashSet, sync::Arc, vec};

    use graph::{
        blockchain::{block_stream::BlockWithTriggers, DataSource as _, TriggersAdapter as _},
        prelude::{tokio, Link},
        semver::Version,
        slog::{self, o, Logger},
    };

    use crate::{
        adapter::{NearReceiptFilter, TriggerFilter},
        codec::{
            self, execution_outcome, receipt, Block, BlockHeader, DataReceiver, ExecutionOutcome,
            ExecutionOutcomeWithId, IndexerExecutionOutcomeWithReceipt, IndexerShard,
            ReceiptAction, SuccessValueExecutionStatus,
        },
        data_source::{DataSource, Mapping, PartialAccounts, ReceiptHandler, NEAR_KIND},
        trigger::{NearTrigger, ReceiptWithOutcome},
        Chain,
    };

    use super::TriggersAdapter;

    #[test]
    fn validate_empty() {
        let ds = new_data_source(None, None);
        let errs = ds.validate();
        assert_eq!(errs.len(), 1, "{:?}", ds);
        assert_eq!(errs[0].to_string(), "subgraph source address is required");
    }

    #[test]
    fn validate_empty_account_none_partial() {
        let ds = new_data_source(None, Some(PartialAccounts::default()));
        let errs = ds.validate();
        assert_eq!(errs.len(), 1, "{:?}", ds);
        assert_eq!(errs[0].to_string(), "subgraph source address is required");
    }

    #[test]
    fn validate_empty_account() {
        let ds = new_data_source(
            None,
            Some(PartialAccounts {
                prefixes: vec![],
                suffixes: vec!["x.near".to_string()],
            }),
        );
        let errs = ds.validate();
        assert_eq!(errs.len(), 0, "{:?}", ds);
    }

    #[test]
    fn validate_empty_prefix_and_suffix_values() {
        let ds = new_data_source(
            None,
            Some(PartialAccounts {
                prefixes: vec!["".to_string()],
                suffixes: vec!["".to_string()],
            }),
        );
        let errs: Vec<String> = ds
            .validate()
            .into_iter()
            .map(|err| err.to_string())
            .collect();
        assert_eq!(errs.len(), 2, "{:?}", ds);

        let expected_errors = vec![
            "partial account prefixes can't have empty values".to_string(),
            "partial account suffixes can't have empty values".to_string(),
        ];
        assert_eq!(
            true,
            expected_errors.iter().all(|err| errs.contains(err)),
            "{:?}",
            errs
        );
    }

    #[test]
    fn validate_empty_partials() {
        let ds = new_data_source(Some("x.near".to_string()), None);
        let errs = ds.validate();
        assert_eq!(errs.len(), 0, "{:?}", ds);
    }

    #[test]
    fn receipt_filter_from_ds() {
        struct Case {
            name: String,
            account: Option<String>,
            partial_accounts: Option<PartialAccounts>,
            expected: HashSet<(Option<String>, Option<String>)>,
        }

        let cases = vec![
            Case {
                name: "2 prefix && 1 suffix".into(),
                account: None,
                partial_accounts: Some(PartialAccounts {
                    prefixes: vec!["a".to_string(), "b".to_string()],
                    suffixes: vec!["d".to_string()],
                }),
                expected: HashSet::from_iter(vec![
                    (Some("a".to_string()), Some("d".to_string())),
                    (Some("b".to_string()), Some("d".to_string())),
                ]),
            },
            Case {
                name: "1 prefix && 2 suffix".into(),
                account: None,
                partial_accounts: Some(PartialAccounts {
                    prefixes: vec!["a".to_string()],
                    suffixes: vec!["c".to_string(), "d".to_string()],
                }),
                expected: HashSet::from_iter(vec![
                    (Some("a".to_string()), Some("c".to_string())),
                    (Some("a".to_string()), Some("d".to_string())),
                ]),
            },
            Case {
                name: "no prefix".into(),
                account: None,
                partial_accounts: Some(PartialAccounts {
                    prefixes: vec![],
                    suffixes: vec!["c".to_string(), "d".to_string()],
                }),
                expected: HashSet::from_iter(vec![
                    (None, Some("c".to_string())),
                    (None, Some("d".to_string())),
                ]),
            },
            Case {
                name: "no suffix".into(),
                account: None,
                partial_accounts: Some(PartialAccounts {
                    prefixes: vec!["a".to_string(), "b".to_string()],
                    suffixes: vec![],
                }),
                expected: HashSet::from_iter(vec![
                    (Some("a".to_string()), None),
                    (Some("b".to_string()), None),
                ]),
            },
        ];

        for case in cases.into_iter() {
            let ds1 = new_data_source(case.account, None);
            let ds2 = new_data_source(None, case.partial_accounts);

            let receipt = NearReceiptFilter::from_data_sources(vec![&ds1, &ds2]);
            assert_eq!(
                receipt.partial_accounts.len(),
                case.expected.len(),
                "name: {}\npartial_accounts: {:?}",
                case.name,
                receipt.partial_accounts,
            );
            assert_eq!(
                true,
                case.expected
                    .iter()
                    .all(|x| receipt.partial_accounts.contains(x)),
                "name: {}\npartial_accounts: {:?}",
                case.name,
                receipt.partial_accounts,
            );
        }
    }

    #[test]
    fn data_source_match_and_decode() {
        struct Request {
            account: String,
            matches: bool,
        }
        struct Case {
            name: String,
            account: Option<String>,
            partial_accounts: Option<PartialAccounts>,
            expected: Vec<Request>,
        }

        let cases = vec![
            Case {
                name: "2 prefix && 1 suffix".into(),
                account: None,
                partial_accounts: Some(PartialAccounts {
                    prefixes: vec!["a".to_string(), "b".to_string()],
                    suffixes: vec!["d".to_string()],
                }),
                expected: vec![
                    Request {
                        account: "ssssssd".to_string(),
                        matches: false,
                    },
                    Request {
                        account: "asasdasdas".to_string(),
                        matches: false,
                    },
                    Request {
                        account: "asd".to_string(),
                        matches: true,
                    },
                    Request {
                        account: "bsd".to_string(),
                        matches: true,
                    },
                ],
            },
            Case {
                name: "1 prefix && 2 suffix".into(),
                account: None,
                partial_accounts: Some(PartialAccounts {
                    prefixes: vec!["a".to_string()],
                    suffixes: vec!["c".to_string(), "d".to_string()],
                }),
                expected: vec![
                    Request {
                        account: "ssssssd".to_string(),
                        matches: false,
                    },
                    Request {
                        account: "asasdasdas".to_string(),
                        matches: false,
                    },
                    Request {
                        account: "asdc".to_string(),
                        matches: true,
                    },
                    Request {
                        account: "absd".to_string(),
                        matches: true,
                    },
                ],
            },
            Case {
                name: "no prefix with exact match".into(),
                account: Some("bsda".to_string()),
                partial_accounts: Some(PartialAccounts {
                    prefixes: vec![],
                    suffixes: vec!["c".to_string(), "d".to_string()],
                }),
                expected: vec![
                    Request {
                        account: "ssssss".to_string(),
                        matches: false,
                    },
                    Request {
                        account: "asasdasdas".to_string(),
                        matches: false,
                    },
                    Request {
                        account: "asdasdasdasdc".to_string(),
                        matches: true,
                    },
                    Request {
                        account: "bsd".to_string(),
                        matches: true,
                    },
                    Request {
                        account: "bsda".to_string(),
                        matches: true,
                    },
                ],
            },
            Case {
                name: "no suffix with exact match".into(),
                account: Some("zbsd".to_string()),
                partial_accounts: Some(PartialAccounts {
                    prefixes: vec!["a".to_string(), "b".to_string()],
                    suffixes: vec![],
                }),
                expected: vec![
                    Request {
                        account: "ssssssd".to_string(),
                        matches: false,
                    },
                    Request {
                        account: "zasdasdas".to_string(),
                        matches: false,
                    },
                    Request {
                        account: "asa".to_string(),
                        matches: true,
                    },
                    Request {
                        account: "bsb".to_string(),
                        matches: true,
                    },
                    Request {
                        account: "zbsd".to_string(),
                        matches: true,
                    },
                ],
            },
        ];

        let logger = Logger::root(slog::Discard, o!());
        for case in cases.into_iter() {
            let ds = new_data_source(case.account, case.partial_accounts);
            let filter = NearReceiptFilter::from_data_sources(vec![&ds]);

            for req in case.expected {
                let res = filter.matches(&req.account);
                assert_eq!(
                    res, req.matches,
                    "name: {} request:{} failed",
                    case.name, req.account
                );

                let block = Arc::new(new_success_block(11, &req.account));
                let receipt = Arc::new(new_receipt_with_outcome(&req.account, block.clone()));
                let res = ds
                    .match_and_decode(&NearTrigger::Receipt(receipt.clone()), &block, &logger)
                    .expect("unable to process block");
                assert_eq!(
                    req.matches,
                    res.is_some(),
                    "case name: {} req: {}",
                    case.name,
                    req.account
                );
            }
        }
    }

    #[tokio::test]
    async fn test_trigger_filter_empty() {
        let account1: String = "account1".into();

        let adapter = TriggersAdapter {};

        let logger = Logger::root(slog::Discard, o!());
        let block1 = new_success_block(1, &account1);

        let filter = TriggerFilter::default();

        let block_with_triggers: BlockWithTriggers<Chain> = adapter
            .triggers_in_block(&logger, block1, &filter)
            .await
            .expect("failed to execute triggers_in_block");
        assert_eq!(block_with_triggers.trigger_count(), 0);
    }

    #[tokio::test]
    async fn test_trigger_filter_every_block() {
        let account1: String = "account1".into();

        let adapter = TriggersAdapter {};

        let logger = Logger::root(slog::Discard, o!());
        let block1 = new_success_block(1, &account1);

        let filter = TriggerFilter {
            block_filter: crate::adapter::NearBlockFilter {
                trigger_every_block: true,
            },
            ..Default::default()
        };

        let block_with_triggers: BlockWithTriggers<Chain> = adapter
            .triggers_in_block(&logger, block1, &filter)
            .await
            .expect("failed to execute triggers_in_block");
        assert_eq!(block_with_triggers.trigger_count(), 1);

        let height: Vec<u64> = heights_from_triggers(&block_with_triggers);
        assert_eq!(height, vec![1]);
    }

    #[tokio::test]
    async fn test_trigger_filter_every_receipt() {
        let account1: String = "account1".into();

        let adapter = TriggersAdapter {};

        let logger = Logger::root(slog::Discard, o!());
        let block1 = new_success_block(1, &account1);

        let filter = TriggerFilter {
            receipt_filter: NearReceiptFilter {
                accounts: HashSet::from_iter(vec![account1]),
                partial_accounts: HashSet::new(),
            },
            ..Default::default()
        };

        let block_with_triggers: BlockWithTriggers<Chain> = adapter
            .triggers_in_block(&logger, block1, &filter)
            .await
            .expect("failed to execute triggers_in_block");
        assert_eq!(block_with_triggers.trigger_count(), 1);

        let height: Vec<u64> = heights_from_triggers(&block_with_triggers);
        assert_eq!(height.len(), 0);
    }

    fn heights_from_triggers(block: &BlockWithTriggers<Chain>) -> Vec<u64> {
        block
            .trigger_data
            .clone()
            .into_iter()
            .filter_map(|x| match x {
                crate::trigger::NearTrigger::Block(b) => b.header.clone().map(|x| x.height),
                _ => None,
            })
            .collect()
    }

    fn new_success_block(height: u64, receiver_id: &String) -> codec::Block {
        codec::Block {
            header: Some(BlockHeader {
                height,
                hash: Some(codec::CryptoHash { bytes: vec![0; 32] }),
                ..Default::default()
            }),
            shards: vec![IndexerShard {
                receipt_execution_outcomes: vec![IndexerExecutionOutcomeWithReceipt {
                    receipt: Some(crate::codec::Receipt {
                        receipt: Some(receipt::Receipt::Action(ReceiptAction {
                            output_data_receivers: vec![DataReceiver {
                                receiver_id: receiver_id.clone(),
                                ..Default::default()
                            }],
                            ..Default::default()
                        })),
                        receiver_id: receiver_id.clone(),
                        ..Default::default()
                    }),
                    execution_outcome: Some(ExecutionOutcomeWithId {
                        outcome: Some(ExecutionOutcome {
                            status: Some(execution_outcome::Status::SuccessValue(
                                SuccessValueExecutionStatus::default(),
                            )),

                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                }],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn new_data_source(
        account: Option<String>,
        partial_accounts: Option<PartialAccounts>,
    ) -> DataSource {
        DataSource {
            kind: NEAR_KIND.to_string(),
            network: None,
            name: "asd".to_string(),
            source: crate::data_source::Source {
                account,
                start_block: 10,
                end_block: None,
                accounts: partial_accounts,
            },
            mapping: Mapping {
                api_version: Version::parse("1.0.0").expect("unable to parse version"),
                language: "".to_string(),
                entities: vec![],
                block_handlers: vec![],
                receipt_handlers: vec![ReceiptHandler {
                    handler: "asdsa".to_string(),
                }],
                runtime: Arc::new(vec![]),
                link: Link::default(),
            },
            context: Arc::new(None),
            creation_block: None,
        }
    }

    fn new_receipt_with_outcome(receiver_id: &String, block: Arc<Block>) -> ReceiptWithOutcome {
        ReceiptWithOutcome {
            outcome: ExecutionOutcomeWithId {
                outcome: Some(ExecutionOutcome {
                    status: Some(execution_outcome::Status::SuccessValue(
                        SuccessValueExecutionStatus::default(),
                    )),

                    ..Default::default()
                }),
                ..Default::default()
            },
            receipt: codec::Receipt {
                receipt: Some(receipt::Receipt::Action(ReceiptAction {
                    output_data_receivers: vec![DataReceiver {
                        receiver_id: receiver_id.clone(),
                        ..Default::default()
                    }],
                    ..Default::default()
                })),
                receiver_id: receiver_id.clone(),
                ..Default::default()
            },
            block,
        }
    }
}
