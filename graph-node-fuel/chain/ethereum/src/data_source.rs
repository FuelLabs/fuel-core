use anyhow::{anyhow, Error};
use anyhow::{ensure, Context};
use graph::blockchain::TriggerWithHandler;
use graph::components::store::StoredDynamicDataSource;
use graph::data_source::CausalityRegion;
use graph::prelude::ethabi::ethereum_types::H160;
use graph::prelude::ethabi::StateMutability;
use graph::prelude::futures03::future::try_join;
use graph::prelude::futures03::stream::FuturesOrdered;
use graph::prelude::{Link, SubgraphManifestValidationError};
use graph::slog::{o, trace};
use std::collections::HashSet;
use std::num::NonZeroU32;
use std::str::FromStr;
use std::sync::Arc;
use tiny_keccak::{keccak256, Keccak};

use graph::{
    blockchain::{self, Blockchain},
    prelude::{
        async_trait,
        ethabi::{Address, Contract, Event, Function, LogParam, ParamType, RawLog},
        serde_json, warn,
        web3::types::{Log, Transaction, H256},
        BlockNumber, CheapClone, DataSourceTemplateInfo, Deserialize, EthereumCall,
        LightEthereumBlock, LightEthereumBlockExt, LinkResolver, Logger, TryStreamExt,
    },
};

use graph::data::subgraph::{
    calls_host_fn, DataSourceContext, Source, MIN_SPEC_VERSION, SPEC_VERSION_0_0_8,
};

use crate::chain::Chain;
use crate::trigger::{EthereumBlockTriggerType, EthereumTrigger, MappingTrigger};

// The recommended kind is `ethereum`, `ethereum/contract` is accepted for backwards compatibility.
const ETHEREUM_KINDS: &[&str] = &["ethereum/contract", "ethereum"];
const EVENT_HANDLER_KIND: &str = "event";
const CALL_HANDLER_KIND: &str = "call";
const BLOCK_HANDLER_KIND: &str = "block";

/// Runtime representation of a data source.
// Note: Not great for memory usage that this needs to be `Clone`, considering how there may be tens
// of thousands of data sources in memory at once.
#[derive(Clone, Debug)]
pub struct DataSource {
    pub kind: String,
    pub network: Option<String>,
    pub name: String,
    pub manifest_idx: u32,
    pub address: Option<Address>,
    pub start_block: BlockNumber,
    pub end_block: Option<BlockNumber>,
    pub mapping: Mapping,
    pub context: Arc<Option<DataSourceContext>>,
    pub creation_block: Option<BlockNumber>,
    pub contract_abi: Arc<MappingABI>,
}

impl blockchain::DataSource<Chain> for DataSource {
    fn from_template_info(info: DataSourceTemplateInfo<Chain>) -> Result<Self, Error> {
        let DataSourceTemplateInfo {
            template,
            params,
            context,
            creation_block,
        } = info;
        let template = template.into_onchain().ok_or(anyhow!(
            "Cannot create onchain data source from offchain template"
        ))?;

        // Obtain the address from the parameters
        let string = params
            .get(0)
            .with_context(|| {
                format!(
                    "Failed to create data source from template `{}`: address parameter is missing",
                    template.name
                )
            })?
            .trim_start_matches("0x");

        let address = Address::from_str(string).with_context(|| {
            format!(
                "Failed to create data source from template `{}`, invalid address provided",
                template.name
            )
        })?;

        let contract_abi = template
            .mapping
            .find_abi(&template.source.abi)
            .with_context(|| format!("template `{}`", template.name))?;

        Ok(DataSource {
            kind: template.kind,
            network: template.network,
            name: template.name,
            manifest_idx: template.manifest_idx,
            address: Some(address),
            start_block: creation_block,
            end_block: None,
            mapping: template.mapping,
            context: Arc::new(context),
            creation_block: Some(creation_block),
            contract_abi,
        })
    }

    fn address(&self) -> Option<&[u8]> {
        self.address.as_ref().map(|x| x.as_bytes())
    }

    fn handler_kinds(&self) -> HashSet<&str> {
        let mut kinds = HashSet::new();

        let Mapping {
            event_handlers,
            call_handlers,
            block_handlers,
            ..
        } = &self.mapping;

        if !event_handlers.is_empty() {
            kinds.insert(EVENT_HANDLER_KIND);
        }
        if !call_handlers.is_empty() {
            kinds.insert(CALL_HANDLER_KIND);
        }
        for handler in block_handlers.iter() {
            kinds.insert(handler.kind());
        }

        kinds
    }

    fn start_block(&self) -> BlockNumber {
        self.start_block
    }

    fn end_block(&self) -> Option<BlockNumber> {
        self.end_block
    }

    fn match_and_decode(
        &self,
        trigger: &<Chain as Blockchain>::TriggerData,
        block: &Arc<<Chain as Blockchain>::Block>,
        logger: &Logger,
    ) -> Result<Option<TriggerWithHandler<Chain>>, Error> {
        let block = block.light_block();
        self.match_and_decode(trigger, block, logger)
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> &str {
        &self.kind
    }

    fn network(&self) -> Option<&str> {
        self.network.as_deref()
    }

    fn context(&self) -> Arc<Option<DataSourceContext>> {
        self.context.cheap_clone()
    }

    fn creation_block(&self) -> Option<BlockNumber> {
        self.creation_block
    }

    fn is_duplicate_of(&self, other: &Self) -> bool {
        let DataSource {
            kind,
            network,
            name,
            manifest_idx,
            address,
            mapping,
            context,
            // The creation block is ignored for detection duplicate data sources.
            // Contract ABI equality is implicit in `mapping.abis` equality.
            creation_block: _,
            contract_abi: _,
            start_block: _,
            end_block: _,
        } = self;

        // mapping_request_sender, host_metrics, and (most of) host_exports are operational structs
        // used at runtime but not needed to define uniqueness; each runtime host should be for a
        // unique data source.
        kind == &other.kind
            && network == &other.network
            && name == &other.name
            && manifest_idx == &other.manifest_idx
            && address == &other.address
            && mapping.abis == other.mapping.abis
            && mapping.event_handlers == other.mapping.event_handlers
            && mapping.call_handlers == other.mapping.call_handlers
            && mapping.block_handlers == other.mapping.block_handlers
            && context == &other.context
    }

    fn as_stored_dynamic_data_source(&self) -> StoredDynamicDataSource {
        let param = self.address.map(|addr| addr.0.into());
        StoredDynamicDataSource {
            manifest_idx: self.manifest_idx,
            param,
            context: self
                .context
                .as_ref()
                .as_ref()
                .map(|ctx| serde_json::to_value(ctx).unwrap()),
            creation_block: self.creation_block,
            done_at: None,
            causality_region: CausalityRegion::ONCHAIN,
        }
    }

    fn from_stored_dynamic_data_source(
        template: &DataSourceTemplate,
        stored: StoredDynamicDataSource,
    ) -> Result<Self, Error> {
        let StoredDynamicDataSource {
            manifest_idx,
            param,
            context,
            creation_block,
            done_at,
            causality_region,
        } = stored;

        ensure!(
            causality_region == CausalityRegion::ONCHAIN,
            "stored ethereum data source has causality region {}, expected root",
            causality_region
        );
        ensure!(done_at.is_none(), "onchain data sources are never done");

        let context = context.map(serde_json::from_value).transpose()?;

        let contract_abi = template.mapping.find_abi(&template.source.abi)?;

        let address = param.map(|x| H160::from_slice(&x));
        Ok(DataSource {
            kind: template.kind.to_string(),
            network: template.network.as_ref().map(|s| s.to_string()),
            name: template.name.clone(),
            manifest_idx,
            address,
            start_block: creation_block.unwrap_or(0),
            end_block: None,
            mapping: template.mapping.clone(),
            context: Arc::new(context),
            creation_block,
            contract_abi,
        })
    }

    fn validate(&self) -> Vec<Error> {
        let mut errors = vec![];

        if !ETHEREUM_KINDS.contains(&self.kind.as_str()) {
            errors.push(anyhow!(
                "data source has invalid `kind`, expected `ethereum` but found {}",
                self.kind
            ))
        }

        // Validate that there is a `source` address if there are call or block handlers
        let no_source_address = self.address().is_none();
        let has_call_handlers = !self.mapping.call_handlers.is_empty();
        let has_block_handlers = !self.mapping.block_handlers.is_empty();
        if no_source_address && (has_call_handlers || has_block_handlers) {
            errors.push(SubgraphManifestValidationError::SourceAddressRequired.into());
        };

        // Ensure that there is at most one instance of each type of block handler
        // and that a combination of a non-filtered block handler and a filtered block handler is not allowed.

        let mut non_filtered_block_handler_count = 0;
        let mut call_filtered_block_handler_count = 0;
        let mut polling_filtered_block_handler_count = 0;
        let mut initialization_handler_count = 0;
        self.mapping
            .block_handlers
            .iter()
            .for_each(|block_handler| {
                match block_handler.filter {
                    None => non_filtered_block_handler_count += 1,
                    Some(ref filter) => match filter {
                        BlockHandlerFilter::Call => call_filtered_block_handler_count += 1,
                        BlockHandlerFilter::Once => initialization_handler_count += 1,
                        BlockHandlerFilter::Polling { every: _ } => {
                            polling_filtered_block_handler_count += 1
                        }
                    },
                };
            });

        let has_non_filtered_block_handler = non_filtered_block_handler_count > 0;
        // If there is a non-filtered block handler, we need to check if there are any
        // filtered block handlers except for the ones with call filter
        // If there are, we do not allow that combination
        let has_restricted_filtered_and_non_filtered_combination = has_non_filtered_block_handler
            && (polling_filtered_block_handler_count > 0 || initialization_handler_count > 0);

        if has_restricted_filtered_and_non_filtered_combination {
            errors.push(anyhow!(
                "data source has a combination of filtered and non-filtered block handlers that is not allowed"
            ));
        }

        // Check the number of handlers for each type
        // If there is more than one of any type, we have too many handlers
        let has_too_many = non_filtered_block_handler_count > 1
            || call_filtered_block_handler_count > 1
            || initialization_handler_count > 1
            || polling_filtered_block_handler_count > 1;

        if has_too_many {
            errors.push(anyhow!("data source has duplicated block handlers"));
        }

        // Validate that event handlers don't require receipts for API versions lower than 0.0.7
        let api_version = self.api_version();
        if api_version < semver::Version::new(0, 0, 7) {
            for event_handler in &self.mapping.event_handlers {
                if event_handler.receipt {
                    errors.push(anyhow!(
                        "data source has event handlers that require transaction receipts, but this \
                         is only supported for apiVersion >= 0.0.7"
                    ));
                    break;
                }
            }
        }

        errors
    }

    fn api_version(&self) -> semver::Version {
        self.mapping.api_version.clone()
    }

    fn min_spec_version(&self) -> semver::Version {
        self.mapping
            .block_handlers
            .iter()
            .fold(MIN_SPEC_VERSION, |mut min, handler| {
                min = match handler.filter {
                    Some(BlockHandlerFilter::Polling { every: _ }) => SPEC_VERSION_0_0_8,
                    Some(BlockHandlerFilter::Once) => SPEC_VERSION_0_0_8,
                    _ => min,
                };
                min
            })
    }

    fn runtime(&self) -> Option<Arc<Vec<u8>>> {
        Some(self.mapping.runtime.cheap_clone())
    }
}

impl DataSource {
    fn from_manifest(
        kind: String,
        network: Option<String>,
        name: String,
        source: Source,
        mapping: Mapping,
        context: Option<DataSourceContext>,
        manifest_idx: u32,
    ) -> Result<Self, Error> {
        // Data sources in the manifest are created "before genesis" so they have no creation block.
        let creation_block = None;
        let contract_abi = mapping
            .find_abi(&source.abi)
            .with_context(|| format!("data source `{}`", name))?;

        Ok(DataSource {
            kind,
            network,
            name,
            manifest_idx,
            address: source.address,
            start_block: source.start_block,
            end_block: source.end_block,
            mapping,
            context: Arc::new(context),
            creation_block,
            contract_abi,
        })
    }

    fn handlers_for_log(&self, log: &Log) -> Vec<MappingEventHandler> {
        // Get signature from the log
        let topic0 = match log.topics.get(0) {
            Some(topic0) => topic0,
            // Events without a topic should just be be ignored
            None => return vec![],
        };

        self.mapping
            .event_handlers
            .iter()
            .filter(|handler| *topic0 == handler.topic0())
            .cloned()
            .collect::<Vec<_>>()
    }

    fn handler_for_call(&self, call: &EthereumCall) -> Result<Option<MappingCallHandler>, Error> {
        // First four bytes of the input for the call are the first four
        // bytes of hash of the function signature
        ensure!(
            call.input.0.len() >= 4,
            "Ethereum call has input with less than 4 bytes"
        );

        let target_method_id = &call.input.0[..4];

        Ok(self
            .mapping
            .call_handlers
            .iter()
            .find(move |handler| {
                let fhash = keccak256(handler.function.as_bytes());
                let actual_method_id = [fhash[0], fhash[1], fhash[2], fhash[3]];
                target_method_id == actual_method_id
            })
            .cloned())
    }

    fn handler_for_block(
        &self,
        trigger_type: &EthereumBlockTriggerType,
        block: BlockNumber,
    ) -> Option<MappingBlockHandler> {
        match trigger_type {
            // Start matches only initialization handlers with a `once` filter
            EthereumBlockTriggerType::Start => self
                .mapping
                .block_handlers
                .iter()
                .find(move |handler| match handler.filter {
                    Some(BlockHandlerFilter::Once) => block == self.start_block,
                    _ => false,
                })
                .cloned(),
            // End matches all handlers without a filter or with a `polling` filter
            EthereumBlockTriggerType::End => self
                .mapping
                .block_handlers
                .iter()
                .find(move |handler| match handler.filter {
                    Some(BlockHandlerFilter::Polling { every }) => {
                        let start_block = self.start_block;
                        let should_trigger = (block - start_block) % every.get() as i32 == 0;
                        should_trigger
                    }
                    None => true,
                    _ => false,
                })
                .cloned(),
            EthereumBlockTriggerType::WithCallTo(_address) => self
                .mapping
                .block_handlers
                .iter()
                .find(move |handler| handler.filter == Some(BlockHandlerFilter::Call))
                .cloned(),
        }
    }

    /// Returns the contract event with the given signature, if it exists. A an event from the ABI
    /// will be matched if:
    /// 1. An event signature is equal to `signature`.
    /// 2. There are no equal matches, but there is exactly one event that equals `signature` if all
    ///    `indexed` modifiers are removed from the parameters.
    fn contract_event_with_signature(&self, signature: &str) -> Option<&Event> {
        // Returns an `Event(uint256,address)` signature for an event, without `indexed` hints.
        fn ambiguous_event_signature(event: &Event) -> String {
            format!(
                "{}({})",
                event.name,
                event
                    .inputs
                    .iter()
                    .map(|input| event_param_type_signature(&input.kind))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }

        // Returns an `Event(indexed uint256,address)` type signature for an event.
        fn event_signature(event: &Event) -> String {
            format!(
                "{}({})",
                event.name,
                event
                    .inputs
                    .iter()
                    .map(|input| format!(
                        "{}{}",
                        if input.indexed { "indexed " } else { "" },
                        event_param_type_signature(&input.kind)
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }

        // Returns the signature of an event parameter type (e.g. `uint256`).
        fn event_param_type_signature(kind: &ParamType) -> String {
            use ParamType::*;

            match kind {
                Address => "address".into(),
                Bytes => "bytes".into(),
                Int(size) => format!("int{}", size),
                Uint(size) => format!("uint{}", size),
                Bool => "bool".into(),
                String => "string".into(),
                Array(inner) => format!("{}[]", event_param_type_signature(inner)),
                FixedBytes(size) => format!("bytes{}", size),
                FixedArray(inner, size) => {
                    format!("{}[{}]", event_param_type_signature(inner), size)
                }
                Tuple(components) => format!(
                    "({})",
                    components
                        .iter()
                        .map(event_param_type_signature)
                        .collect::<Vec<_>>()
                        .join(",")
                ),
            }
        }

        self.contract_abi
            .contract
            .events()
            .find(|event| event_signature(event) == signature)
            .or_else(|| {
                // Fallback for subgraphs that don't use `indexed` in event signatures yet:
                //
                // If there is only one event variant with this name and if its signature
                // without `indexed` matches the event signature from the manifest, we
                // can safely assume that the event is a match, we don't need to force
                // the subgraph to add `indexed`.

                // Extract the event name; if there is no '(' in the signature,
                // `event_name` will be empty and not match any events, so that's ok
                let parens = signature.find('(').unwrap_or(0);
                let event_name = &signature[0..parens];

                let matching_events = self
                    .contract_abi
                    .contract
                    .events()
                    .filter(|event| event.name == event_name)
                    .collect::<Vec<_>>();

                // Only match the event signature without `indexed` if there is
                // only a single event variant
                if matching_events.len() == 1
                    && ambiguous_event_signature(matching_events[0]) == signature
                {
                    Some(matching_events[0])
                } else {
                    // More than one event variant or the signature
                    // still doesn't match, even if we ignore `indexed` hints
                    None
                }
            })
    }

    fn contract_function_with_signature(&self, target_signature: &str) -> Option<&Function> {
        self.contract_abi
            .contract
            .functions()
            .filter(|function| match function.state_mutability {
                StateMutability::Payable | StateMutability::NonPayable => true,
                StateMutability::Pure | StateMutability::View => false,
            })
            .find(|function| {
                // Construct the argument function signature:
                // `address,uint256,bool`
                let mut arguments = function
                    .inputs
                    .iter()
                    .map(|input| format!("{}", input.kind))
                    .collect::<Vec<String>>()
                    .join(",");
                // `address,uint256,bool)
                arguments.push(')');
                // `operation(address,uint256,bool)`
                let actual_signature = vec![function.name.clone(), arguments].join("(");
                target_signature == actual_signature
            })
    }

    fn matches_trigger_address(&self, trigger: &EthereumTrigger) -> bool {
        let Some(ds_address) = self.address else {
            // 'wildcard' data sources match any trigger address.
            return true
        };

        let Some(trigger_address) = trigger.address() else {
             return true
        };

        ds_address == *trigger_address
    }

    /// Checks if `trigger` matches this data source, and if so decodes it into a `MappingTrigger`.
    /// A return of `Ok(None)` mean the trigger does not match.
    fn match_and_decode(
        &self,
        trigger: &EthereumTrigger,
        block: &Arc<LightEthereumBlock>,
        logger: &Logger,
    ) -> Result<Option<TriggerWithHandler<Chain>>, Error> {
        if !self.matches_trigger_address(trigger) {
            return Ok(None);
        }

        if self.start_block > block.number() {
            return Ok(None);
        }

        match trigger {
            EthereumTrigger::Block(_, trigger_type) => {
                let handler = match self.handler_for_block(trigger_type, block.number()) {
                    Some(handler) => handler,
                    None => return Ok(None),
                };
                Ok(Some(TriggerWithHandler::<Chain>::new(
                    MappingTrigger::Block {
                        block: block.cheap_clone(),
                    },
                    handler.handler,
                    block.block_ptr(),
                )))
            }
            EthereumTrigger::Log(log_ref) => {
                let log = Arc::new(log_ref.log().clone());
                let receipt = log_ref.receipt();
                let potential_handlers = self.handlers_for_log(&log);

                // Map event handlers to (event handler, event ABI) pairs; fail if there are
                // handlers that don't exist in the contract ABI
                let valid_handlers = potential_handlers
                    .into_iter()
                    .map(|event_handler| {
                        // Identify the event ABI in the contract
                        let event_abi = self
                            .contract_event_with_signature(event_handler.event.as_str())
                            .with_context(|| {
                                anyhow!(
                                    "Event with the signature \"{}\" not found in \
                                            contract \"{}\" of data source \"{}\"",
                                    event_handler.event,
                                    self.contract_abi.name,
                                    self.name,
                                )
                            })?;
                        Ok((event_handler, event_abi))
                    })
                    .collect::<Result<Vec<_>, anyhow::Error>>()?;

                // Filter out handlers whose corresponding event ABIs cannot decode the
                // params (this is common for overloaded events that have the same topic0
                // but have indexed vs. non-indexed params that are encoded differently).
                //
                // Map (handler, event ABI) pairs to (handler, decoded params) pairs.
                let mut matching_handlers = valid_handlers
                    .into_iter()
                    .filter_map(|(event_handler, event_abi)| {
                        event_abi
                            .parse_log(RawLog {
                                topics: log.topics.clone(),
                                data: log.data.clone().0,
                            })
                            .map(|log| log.params)
                            .map_err(|e| {
                                trace!(
                                    logger,
                                    "Skipping handler because the event parameters do not \
                                    match the event signature. This is typically the case \
                                    when parameters are indexed in the event but not in the \
                                    signature or the other way around";
                                    "handler" => &event_handler.handler,
                                    "event" => &event_handler.event,
                                    "error" => format!("{}", e),
                                );
                            })
                            .ok()
                            .map(|params| (event_handler, params))
                    })
                    .collect::<Vec<_>>();

                if matching_handlers.is_empty() {
                    return Ok(None);
                }

                // Process the event with the matching handler
                let (event_handler, params) = matching_handlers.pop().unwrap();

                ensure!(
                    matching_handlers.is_empty(),
                    format!(
                        "Multiple handlers defined for event `{}`, only one is supported",
                        &event_handler.event
                    )
                );

                // Special case: In Celo, there are Epoch Rewards events, which do not have an
                // associated transaction and instead have `transaction_hash == block.hash`,
                // in which case we pass a dummy transaction to the mappings.
                // See also ca0edc58-0ec5-4c89-a7dd-2241797f5e50.
                let transaction = if log.transaction_hash != block.hash {
                    block
                        .transaction_for_log(&log)
                        .context("Found no transaction for event")?
                } else {
                    // Infer some fields from the log and fill the rest with zeros.
                    Transaction {
                        hash: log.transaction_hash.unwrap(),
                        block_hash: block.hash,
                        block_number: block.number,
                        transaction_index: log.transaction_index,
                        from: Some(H160::zero()),
                        ..Transaction::default()
                    }
                };

                let logging_extras = Arc::new(o! {
                    "signature" => event_handler.event.to_string(),
                    "address" => format!("{}", &log.address),
                    "transaction" => format!("{}", &transaction.hash),
                });
                Ok(Some(TriggerWithHandler::<Chain>::new_with_logging_extras(
                    MappingTrigger::Log {
                        block: block.cheap_clone(),
                        transaction: Arc::new(transaction),
                        log: log,
                        params,
                        receipt: receipt.map(|r| r.cheap_clone()),
                    },
                    event_handler.handler,
                    block.block_ptr(),
                    logging_extras,
                )))
            }
            EthereumTrigger::Call(call) => {
                // Identify the call handler for this call
                let handler = match self.handler_for_call(call)? {
                    Some(handler) => handler,
                    None => return Ok(None),
                };

                // Identify the function ABI in the contract
                let function_abi = self
                    .contract_function_with_signature(handler.function.as_str())
                    .with_context(|| {
                        anyhow!(
                            "Function with the signature \"{}\" not found in \
                    contract \"{}\" of data source \"{}\"",
                            handler.function,
                            self.contract_abi.name,
                            self.name
                        )
                    })?;

                // Parse the inputs
                //
                // Take the input for the call, chop off the first 4 bytes, then call
                // `function.decode_input` to get a vector of `Token`s. Match the `Token`s
                // with the `Param`s in `function.inputs` to create a `Vec<LogParam>`.
                let tokens = match function_abi.decode_input(&call.input.0[4..]).with_context(
                    || {
                        format!(
                            "Generating function inputs for the call {:?} failed, raw input: {}",
                            &function_abi,
                            hex::encode(&call.input.0)
                        )
                    },
                ) {
                    Ok(val) => val,
                    // See also 280b0108-a96e-4738-bb37-60ce11eeb5bf
                    Err(err) => {
                        warn!(logger, "Failed parsing inputs, skipping"; "error" => &err.to_string());
                        return Ok(None);
                    }
                };

                ensure!(
                    tokens.len() == function_abi.inputs.len(),
                    "Number of arguments in call does not match \
                    number of inputs in function signature."
                );

                let inputs = tokens
                    .into_iter()
                    .enumerate()
                    .map(|(i, token)| LogParam {
                        name: function_abi.inputs[i].name.clone(),
                        value: token,
                    })
                    .collect::<Vec<_>>();

                // Parse the outputs
                //
                // Take the output for the call, then call `function.decode_output` to
                // get a vector of `Token`s. Match the `Token`s with the `Param`s in
                // `function.outputs` to create a `Vec<LogParam>`.
                let tokens = function_abi
                    .decode_output(&call.output.0)
                    .with_context(|| {
                        format!(
                            "Decoding function outputs for the call {:?} failed, raw output: {}",
                            &function_abi,
                            hex::encode(&call.output.0)
                        )
                    })?;

                ensure!(
                    tokens.len() == function_abi.outputs.len(),
                    "Number of parameters in the call output does not match \
                        number of outputs in the function signature."
                );

                let outputs = tokens
                    .into_iter()
                    .enumerate()
                    .map(|(i, token)| LogParam {
                        name: function_abi.outputs[i].name.clone(),
                        value: token,
                    })
                    .collect::<Vec<_>>();

                let transaction = Arc::new(
                    block
                        .transaction_for_call(call)
                        .context("Found no transaction for call")?,
                );
                let logging_extras = Arc::new(o! {
                    "function" => handler.function.to_string(),
                    "to" => format!("{}", &call.to),
                    "transaction" => format!("{}", &transaction.hash),
                });
                Ok(Some(TriggerWithHandler::<Chain>::new_with_logging_extras(
                    MappingTrigger::Call {
                        block: block.cheap_clone(),
                        transaction,
                        call: call.cheap_clone(),
                        inputs,
                        outputs,
                    },
                    handler.handler,
                    block.block_ptr(),
                    logging_extras,
                )))
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
pub struct UnresolvedDataSource {
    pub kind: String,
    pub network: Option<String>,
    pub name: String,
    pub source: Source,
    pub mapping: UnresolvedMapping,
    pub context: Option<DataSourceContext>,
}

#[async_trait]
impl blockchain::UnresolvedDataSource<Chain> for UnresolvedDataSource {
    async fn resolve(
        self,
        resolver: &Arc<dyn LinkResolver>,
        logger: &Logger,
        manifest_idx: u32,
    ) -> Result<DataSource, anyhow::Error> {
        let UnresolvedDataSource {
            kind,
            network,
            name,
            source,
            mapping,
            context,
        } = self;

        let mapping = mapping.resolve(resolver, logger).await.with_context(|| {
            format!(
                "failed to resolve data source {} with source_address {:?} and source_start_block {}",
                name, source.address, source.start_block
            )
        })?;

        DataSource::from_manifest(kind, network, name, source, mapping, context, manifest_idx)
    }
}

#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Deserialize)]
pub struct UnresolvedDataSourceTemplate {
    pub kind: String,
    pub network: Option<String>,
    pub name: String,
    pub source: TemplateSource,
    pub mapping: UnresolvedMapping,
}

#[derive(Clone, Debug)]
pub struct DataSourceTemplate {
    pub kind: String,
    pub network: Option<String>,
    pub name: String,
    pub manifest_idx: u32,
    pub source: TemplateSource,
    pub mapping: Mapping,
}

#[async_trait]
impl blockchain::UnresolvedDataSourceTemplate<Chain> for UnresolvedDataSourceTemplate {
    async fn resolve(
        self,
        resolver: &Arc<dyn LinkResolver>,
        logger: &Logger,
        manifest_idx: u32,
    ) -> Result<DataSourceTemplate, anyhow::Error> {
        let UnresolvedDataSourceTemplate {
            kind,
            network,
            name,
            source,
            mapping,
        } = self;

        let mapping = mapping
            .resolve(resolver, logger)
            .await
            .with_context(|| format!("failed to resolve data source template {}", name))?;

        Ok(DataSourceTemplate {
            kind,
            network,
            name,
            manifest_idx,
            source,
            mapping,
        })
    }
}

impl blockchain::DataSourceTemplate<Chain> for DataSourceTemplate {
    fn name(&self) -> &str {
        &self.name
    }

    fn api_version(&self) -> semver::Version {
        self.mapping.api_version.clone()
    }

    fn runtime(&self) -> Option<Arc<Vec<u8>>> {
        Some(self.mapping.runtime.cheap_clone())
    }

    fn manifest_idx(&self) -> u32 {
        self.manifest_idx
    }

    fn kind(&self) -> &str {
        &self.kind
    }
}

#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedMapping {
    pub kind: String,
    pub api_version: String,
    pub language: String,
    pub entities: Vec<String>,
    pub abis: Vec<UnresolvedMappingABI>,
    #[serde(default)]
    pub block_handlers: Vec<MappingBlockHandler>,
    #[serde(default)]
    pub call_handlers: Vec<MappingCallHandler>,
    #[serde(default)]
    pub event_handlers: Vec<MappingEventHandler>,
    pub file: Link,
}

#[derive(Clone, Debug)]
pub struct Mapping {
    pub kind: String,
    pub api_version: semver::Version,
    pub language: String,
    pub entities: Vec<String>,
    pub abis: Vec<Arc<MappingABI>>,
    pub block_handlers: Vec<MappingBlockHandler>,
    pub call_handlers: Vec<MappingCallHandler>,
    pub event_handlers: Vec<MappingEventHandler>,
    pub runtime: Arc<Vec<u8>>,
    pub link: Link,
}

impl Mapping {
    pub fn requires_archive(&self) -> anyhow::Result<bool> {
        calls_host_fn(&self.runtime, "ethereum.call")
    }

    pub fn has_call_handler(&self) -> bool {
        !self.call_handlers.is_empty()
    }

    pub fn has_block_handler_with_call_filter(&self) -> bool {
        self.block_handlers
            .iter()
            .any(|handler| matches!(handler.filter, Some(BlockHandlerFilter::Call)))
    }

    pub fn find_abi(&self, abi_name: &str) -> Result<Arc<MappingABI>, Error> {
        Ok(self
            .abis
            .iter()
            .find(|abi| abi.name == abi_name)
            .ok_or_else(|| anyhow!("No ABI entry with name `{}` found", abi_name))?
            .cheap_clone())
    }
}

impl UnresolvedMapping {
    pub async fn resolve(
        self,
        resolver: &Arc<dyn LinkResolver>,
        logger: &Logger,
    ) -> Result<Mapping, anyhow::Error> {
        let UnresolvedMapping {
            kind,
            api_version,
            language,
            entities,
            abis,
            block_handlers,
            call_handlers,
            event_handlers,
            file: link,
        } = self;

        let api_version = semver::Version::parse(&api_version)?;

        let (abis, runtime) = try_join(
            // resolve each abi
            abis.into_iter()
                .map(|unresolved_abi| async {
                    Result::<_, Error>::Ok(Arc::new(
                        unresolved_abi.resolve(resolver, logger).await?,
                    ))
                })
                .collect::<FuturesOrdered<_>>()
                .try_collect::<Vec<_>>(),
            async {
                let module_bytes = resolver.cat(logger, &link).await?;
                Ok(Arc::new(module_bytes))
            },
        )
        .await
        .with_context(|| format!("failed to resolve mapping {}", link.link))?;

        Ok(Mapping {
            kind,
            api_version,
            language,
            entities,
            abis,
            block_handlers: block_handlers.clone(),
            call_handlers: call_handlers.clone(),
            event_handlers: event_handlers.clone(),
            runtime,
            link,
        })
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize)]
pub struct UnresolvedMappingABI {
    pub name: String,
    pub file: Link,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MappingABI {
    pub name: String,
    pub contract: Contract,
}

impl UnresolvedMappingABI {
    pub async fn resolve(
        self,
        resolver: &Arc<dyn LinkResolver>,
        logger: &Logger,
    ) -> Result<MappingABI, anyhow::Error> {
        let contract_bytes = resolver.cat(logger, &self.file).await.with_context(|| {
            format!(
                "failed to resolve ABI {} from {}",
                self.name, self.file.link
            )
        })?;
        let contract = Contract::load(&*contract_bytes)?;
        Ok(MappingABI {
            name: self.name,
            contract,
        })
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize)]
pub struct MappingBlockHandler {
    pub handler: String,
    pub filter: Option<BlockHandlerFilter>,
}

impl MappingBlockHandler {
    pub fn kind(&self) -> &str {
        match &self.filter {
            Some(filter) => match filter {
                BlockHandlerFilter::Call => "block_filter_call",
                BlockHandlerFilter::Once => "block_filter_once",
                BlockHandlerFilter::Polling { .. } => "block_filter_polling",
            },
            None => BLOCK_HANDLER_KIND,
        }
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum BlockHandlerFilter {
    // Call filter will trigger on all blocks where the data source contract
    // address has been called
    Call,
    // This filter will trigger once at the startBlock
    Once,
    // This filter will trigger in a recurring interval set by the `every` field.
    Polling { every: NonZeroU32 },
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize)]
pub struct MappingCallHandler {
    pub function: String,
    pub handler: String,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize)]
pub struct MappingEventHandler {
    pub event: String,
    pub topic0: Option<H256>,
    pub handler: String,
    #[serde(default)]
    pub receipt: bool,
}

impl MappingEventHandler {
    pub fn topic0(&self) -> H256 {
        self.topic0
            .unwrap_or_else(|| string_to_h256(&self.event.replace("indexed ", "")))
    }
}

/// Hashes a string to a H256 hash.
fn string_to_h256(s: &str) -> H256 {
    let mut result = [0u8; 32];
    let data = s.replace(' ', "").into_bytes();
    let mut sponge = Keccak::new_keccak256();
    sponge.update(&data);
    sponge.finalize(&mut result);

    // This was deprecated but the replacement seems to not be available in the
    // version web3 uses.
    #[allow(deprecated)]
    H256::from_slice(&result)
}

#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Deserialize)]
pub struct TemplateSource {
    pub abi: String,
}
