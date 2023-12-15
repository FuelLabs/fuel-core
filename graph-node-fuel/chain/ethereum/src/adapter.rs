use anyhow::Error;
use ethabi::{Error as ABIError, Function, ParamType, Token};
use futures::Future;
use graph::blockchain::ChainIdentifier;
use graph::firehose::CallToFilter;
use graph::firehose::CombinedFilter;
use graph::firehose::LogFilter;
use itertools::Itertools;
use prost::Message;
use prost_types::Any;
use std::cmp;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::marker::Unpin;
use thiserror::Error;
use tiny_keccak::keccak256;
use web3::types::{Address, Log, H256};

use graph::prelude::*;
use graph::{
    blockchain as bc,
    components::metrics::{CounterVec, GaugeVec, HistogramVec},
    petgraph::{self, graphmap::GraphMap},
};

const COMBINED_FILTER_TYPE_URL: &str =
    "type.googleapis.com/sf.ethereum.transform.v1.CombinedFilter";

use crate::capabilities::NodeCapabilities;
use crate::data_source::{BlockHandlerFilter, DataSource};
use crate::{Chain, Mapping, ENV_VARS};

pub type EventSignature = H256;
pub type FunctionSelector = [u8; 4];

#[derive(Clone, Debug)]
pub struct EthereumContractCall {
    pub address: Address,
    pub block_ptr: BlockPtr,
    pub function: Function,
    pub args: Vec<Token>,
    pub gas: Option<u32>,
}

#[derive(Error, Debug)]
pub enum EthereumContractCallError {
    #[error("ABI error: {0}")]
    ABIError(#[from] ABIError),
    /// `Token` is not of expected `ParamType`
    #[error("type mismatch, token {0:?} is not of kind {1:?}")]
    TypeError(Token, ParamType),
    #[error("error encoding input call data: {0}")]
    EncodingError(ethabi::Error),
    #[error("call error: {0}")]
    Web3Error(web3::Error),
    #[error("call reverted: {0}")]
    Revert(String),
    #[error("ethereum node took too long to perform call")]
    Timeout,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
enum LogFilterNode {
    Contract(Address),
    Event(EventSignature),
}

/// Corresponds to an `eth_getLogs` call.
#[derive(Clone, Debug)]
pub struct EthGetLogsFilter {
    pub contracts: Vec<Address>,
    pub event_signatures: Vec<EventSignature>,
}

impl EthGetLogsFilter {
    fn from_contract(address: Address) -> Self {
        EthGetLogsFilter {
            contracts: vec![address],
            event_signatures: vec![],
        }
    }

    fn from_event(event: EventSignature) -> Self {
        EthGetLogsFilter {
            contracts: vec![],
            event_signatures: vec![event],
        }
    }
}

impl fmt::Display for EthGetLogsFilter {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.contracts.len() == 1 {
            write!(
                f,
                "contract {:?}, {} events",
                self.contracts[0],
                self.event_signatures.len()
            )
        } else if self.event_signatures.len() == 1 {
            write!(
                f,
                "event {:?}, {} contracts",
                self.event_signatures[0],
                self.contracts.len()
            )
        } else {
            write!(f, "unreachable")
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct TriggerFilter {
    pub(crate) log: EthereumLogFilter,
    pub(crate) call: EthereumCallFilter,
    pub(crate) block: EthereumBlockFilter,
}

impl TriggerFilter {
    pub(crate) fn requires_traces(&self) -> bool {
        !self.call.is_empty() || self.block.requires_traces()
    }

    #[cfg(debug_assertions)]
    pub fn log(&self) -> &EthereumLogFilter {
        &self.log
    }

    #[cfg(debug_assertions)]
    pub fn call(&self) -> &EthereumCallFilter {
        &self.call
    }

    #[cfg(debug_assertions)]
    pub fn block(&self) -> &EthereumBlockFilter {
        &self.block
    }
}

impl bc::TriggerFilter<Chain> for TriggerFilter {
    fn extend<'a>(&mut self, data_sources: impl Iterator<Item = &'a DataSource> + Clone) {
        self.log
            .extend(EthereumLogFilter::from_data_sources(data_sources.clone()));
        self.call
            .extend(EthereumCallFilter::from_data_sources(data_sources.clone()));
        self.block
            .extend(EthereumBlockFilter::from_data_sources(data_sources));
    }

    fn node_capabilities(&self) -> NodeCapabilities {
        NodeCapabilities {
            archive: false,
            traces: self.requires_traces(),
        }
    }

    fn extend_with_template(
        &mut self,
        data_sources: impl Iterator<Item = <Chain as bc::Blockchain>::DataSourceTemplate>,
    ) {
        for data_source in data_sources {
            self.log
                .extend(EthereumLogFilter::from_mapping(&data_source.mapping));

            self.call
                .extend(EthereumCallFilter::from_mapping(&data_source.mapping));

            self.block
                .extend(EthereumBlockFilter::from_mapping(&data_source.mapping));
        }
    }

    fn to_firehose_filter(self) -> Vec<prost_types::Any> {
        let EthereumBlockFilter {
            polling_intervals,
            contract_addresses: _contract_addresses,
            trigger_every_block,
        } = self.block.clone();

        let log_filters: Vec<LogFilter> = self.log.into();
        let mut call_filters: Vec<CallToFilter> = self.call.into();
        call_filters.extend(Into::<Vec<CallToFilter>>::into(self.block));

        if call_filters.is_empty() && log_filters.is_empty() && !trigger_every_block {
            return Vec::new();
        }

        let combined_filter = CombinedFilter {
            log_filters,
            call_filters,
            send_all_block_headers: trigger_every_block || !polling_intervals.is_empty(),
        };

        vec![Any {
            type_url: COMBINED_FILTER_TYPE_URL.into(),
            value: combined_filter.encode_to_vec(),
        }]
    }
}

#[derive(Clone, Debug, Default)]
pub struct EthereumLogFilter {
    /// Log filters can be represented as a bipartite graph between contracts and events. An edge
    /// exists between a contract and an event if a data source for the contract has a trigger for
    /// the event.
    /// Edges are of `bool` type and indicates when a trigger requires a transaction receipt.
    contracts_and_events_graph: GraphMap<LogFilterNode, bool, petgraph::Undirected>,

    /// Event sigs with no associated address, matching on all addresses.
    /// Maps to a boolean representing if a trigger requires a transaction receipt.
    wildcard_events: HashMap<EventSignature, bool>,
}

impl From<EthereumLogFilter> for Vec<LogFilter> {
    fn from(val: EthereumLogFilter) -> Self {
        val.eth_get_logs_filters()
            .map(
                |EthGetLogsFilter {
                     contracts,
                     event_signatures,
                 }| LogFilter {
                    addresses: contracts
                        .iter()
                        .map(|addr| addr.to_fixed_bytes().to_vec())
                        .collect_vec(),
                    event_signatures: event_signatures
                        .iter()
                        .map(|sig| sig.to_fixed_bytes().to_vec())
                        .collect_vec(),
                },
            )
            .collect_vec()
    }
}

impl EthereumLogFilter {
    /// Check if this filter matches the specified `Log`.
    pub fn matches(&self, log: &Log) -> bool {
        // First topic should be event sig
        match log.topics.first() {
            None => false,

            Some(sig) => {
                // The `Log` matches the filter either if the filter contains
                // a (contract address, event signature) pair that matches the
                // `Log`, or if the filter contains wildcard event that matches.
                let contract = LogFilterNode::Contract(log.address);
                let event = LogFilterNode::Event(*sig);
                self.contracts_and_events_graph
                    .all_edges()
                    .any(|(s, t, _)| (s == contract && t == event) || (t == contract && s == event))
                    || self.wildcard_events.contains_key(sig)
            }
        }
    }

    /// Similar to [`matches`], checks if a transaction receipt is required for this log filter.
    pub fn requires_transaction_receipt(
        &self,
        event_signature: &H256,
        contract_address: Option<&Address>,
    ) -> bool {
        if let Some(true) = self.wildcard_events.get(event_signature) {
            true
        } else if let Some(address) = contract_address {
            let contract = LogFilterNode::Contract(*address);
            let event = LogFilterNode::Event(*event_signature);
            self.contracts_and_events_graph
                .all_edges()
                .any(|(s, t, r)| {
                    *r && (s == contract && t == event) || (t == contract && s == event)
                })
        } else {
            false
        }
    }

    pub fn from_data_sources<'a>(iter: impl IntoIterator<Item = &'a DataSource>) -> Self {
        let mut this = EthereumLogFilter::default();
        for ds in iter {
            for event_handler in ds.mapping.event_handlers.iter() {
                let event_sig = event_handler.topic0();
                match ds.address {
                    Some(contract) => {
                        this.contracts_and_events_graph.add_edge(
                            LogFilterNode::Contract(contract),
                            LogFilterNode::Event(event_sig),
                            event_handler.receipt,
                        );
                    }
                    None => {
                        this.wildcard_events
                            .insert(event_sig, event_handler.receipt);
                    }
                }
            }
        }
        this
    }

    pub fn from_mapping(mapping: &Mapping) -> Self {
        let mut this = EthereumLogFilter::default();
        for event_handler in &mapping.event_handlers {
            let signature = event_handler.topic0();
            this.wildcard_events
                .insert(signature, event_handler.receipt);
        }
        this
    }

    /// Extends this log filter with another one.
    pub fn extend(&mut self, other: EthereumLogFilter) {
        if other.is_empty() {
            return;
        };

        // Destructure to make sure we're checking all fields.
        let EthereumLogFilter {
            contracts_and_events_graph,
            wildcard_events,
        } = other;
        for (s, t, e) in contracts_and_events_graph.all_edges() {
            self.contracts_and_events_graph.add_edge(s, t, *e);
        }
        self.wildcard_events.extend(wildcard_events);
    }

    /// An empty filter is one that never matches.
    pub fn is_empty(&self) -> bool {
        // Destructure to make sure we're checking all fields.
        let EthereumLogFilter {
            contracts_and_events_graph,
            wildcard_events,
        } = self;
        contracts_and_events_graph.edge_count() == 0 && wildcard_events.is_empty()
    }

    /// Filters for `eth_getLogs` calls. The filters will not return false positives. This attempts
    /// to balance between having granular filters but too many calls and having few calls but too
    /// broad filters causing the Ethereum endpoint to timeout.
    pub fn eth_get_logs_filters(self) -> impl Iterator<Item = EthGetLogsFilter> {
        // Start with the wildcard event filters.
        let mut filters = self
            .wildcard_events
            .into_keys()
            .map(EthGetLogsFilter::from_event)
            .collect_vec();

        // The current algorithm is to repeatedly find the maximum cardinality vertex and turn all
        // of its edges into a filter. This is nice because it is neutral between filtering by
        // contract or by events, if there are many events that appear on only one data source
        // we'll filter by many events on a single contract, but if there is an event that appears
        // on a lot of data sources we'll filter by many contracts with a single event.
        //
        // From a theoretical standpoint we're finding a vertex cover, and this is not the optimal
        // algorithm to find a minimum vertex cover, but should be fine as an approximation.
        //
        // One optimization we're not doing is to merge nodes that have the same neighbors into a
        // single node. For example if a subgraph has two data sources, each with the same two
        // events, we could cover that with a single filter and no false positives. However that
        // might cause the filter to become too broad, so at the moment it seems excessive.
        let mut g = self.contracts_and_events_graph;
        while g.edge_count() > 0 {
            let mut push_filter = |filter: EthGetLogsFilter| {
                // Sanity checks:
                // - The filter is not a wildcard because all nodes have neighbors.
                // - The graph is bipartite.
                assert!(!filter.contracts.is_empty() && !filter.event_signatures.is_empty());
                assert!(filter.contracts.len() == 1 || filter.event_signatures.len() == 1);
                filters.push(filter);
            };

            // If there are edges, there are vertexes.
            let max_vertex = g.nodes().max_by_key(|&n| g.neighbors(n).count()).unwrap();
            let mut filter = match max_vertex {
                LogFilterNode::Contract(address) => EthGetLogsFilter::from_contract(address),
                LogFilterNode::Event(event_sig) => EthGetLogsFilter::from_event(event_sig),
            };
            for neighbor in g.neighbors(max_vertex) {
                match neighbor {
                    LogFilterNode::Contract(address) => {
                        if filter.contracts.len() == ENV_VARS.get_logs_max_contracts {
                            // The batch size was reached, register the filter and start a new one.
                            let event = filter.event_signatures[0];
                            push_filter(filter);
                            filter = EthGetLogsFilter::from_event(event);
                        }
                        filter.contracts.push(address);
                    }
                    LogFilterNode::Event(event_sig) => filter.event_signatures.push(event_sig),
                }
            }

            push_filter(filter);
            g.remove_node(max_vertex);
        }
        filters.into_iter()
    }

    #[cfg(debug_assertions)]
    pub fn contract_addresses(&self) -> impl Iterator<Item = Address> + '_ {
        self.contracts_and_events_graph
            .nodes()
            .filter_map(|node| match node {
                LogFilterNode::Contract(address) => Some(address),
                LogFilterNode::Event(_) => None,
            })
    }
}

#[derive(Clone, Debug, Default)]
pub struct EthereumCallFilter {
    // Each call filter has a map of filters keyed by address, each containing a tuple with
    // start_block and the set of function signatures
    pub contract_addresses_function_signatures:
        HashMap<Address, (BlockNumber, HashSet<FunctionSelector>)>,

    pub wildcard_signatures: HashSet<FunctionSelector>,
}

impl Into<Vec<CallToFilter>> for EthereumCallFilter {
    fn into(self) -> Vec<CallToFilter> {
        if self.is_empty() {
            return Vec::new();
        }

        let EthereumCallFilter {
            contract_addresses_function_signatures,
            wildcard_signatures,
        } = self;

        let mut filters: Vec<CallToFilter> = contract_addresses_function_signatures
            .into_iter()
            .map(|(addr, (_, sigs))| CallToFilter {
                addresses: vec![addr.to_fixed_bytes().to_vec()],
                signatures: sigs.into_iter().map(|x| x.to_vec()).collect_vec(),
            })
            .collect();

        if !wildcard_signatures.is_empty() {
            filters.push(CallToFilter {
                addresses: vec![],
                signatures: wildcard_signatures
                    .into_iter()
                    .map(|x| x.to_vec())
                    .collect_vec(),
            });
        }

        filters
    }
}

impl EthereumCallFilter {
    pub fn matches(&self, call: &EthereumCall) -> bool {
        // Calls returned by Firehose actually contains pure transfers and smart
        // contract calls. If the input is less than 4 bytes, we assume it's a pure transfer
        // and discards those.
        if call.input.0.len() < 4 {
            return false;
        }

        // The `call.input.len()` is validated in the
        // DataSource::match_and_decode function.
        // Those calls are logged as warning and skipped.
        //
        // See 280b0108-a96e-4738-bb37-60ce11eeb5bf
        let call_signature = &call.input.0[..4];

        // Ensure the call is to a contract the filter expressed an interest in
        match self.contract_addresses_function_signatures.get(&call.to) {
            // If the call is to a contract with no specified functions, keep the call
            //
            // Allows the ability to genericly match on all calls to a contract.
            // Caveat is this catch all clause limits you from matching with a specific call
            // on the same address
            Some(v) if v.1.is_empty() => true,
            // There are some relevant signatures to test
            // this avoids having to call extend for every match call, checks the contract specific funtions, then falls
            // back on wildcards
            Some(v) => {
                let sig = &v.1;
                sig.contains(call_signature) || self.wildcard_signatures.contains(call_signature)
            }
            // no contract specific functions, check wildcards
            None => self.wildcard_signatures.contains(call_signature),
        }
    }

    pub fn from_mapping(mapping: &Mapping) -> Self {
        let functions = mapping
            .call_handlers
            .iter()
            .map(move |call_handler| {
                let sig = keccak256(call_handler.function.as_bytes());
                [sig[0], sig[1], sig[2], sig[3]]
            })
            .collect();

        Self {
            wildcard_signatures: functions,
            contract_addresses_function_signatures: HashMap::new(),
        }
    }

    pub fn from_data_sources<'a>(iter: impl IntoIterator<Item = &'a DataSource>) -> Self {
        iter.into_iter()
            .filter_map(|data_source| data_source.address.map(|addr| (addr, data_source)))
            .flat_map(|(contract_addr, data_source)| {
                let start_block = data_source.start_block;
                data_source
                    .mapping
                    .call_handlers
                    .iter()
                    .map(move |call_handler| {
                        let sig = keccak256(call_handler.function.as_bytes());
                        (start_block, contract_addr, [sig[0], sig[1], sig[2], sig[3]])
                    })
            })
            .collect()
    }

    /// Extends this call filter with another one.
    pub fn extend(&mut self, other: EthereumCallFilter) {
        if other.is_empty() {
            return;
        };

        let EthereumCallFilter {
            contract_addresses_function_signatures,
            wildcard_signatures,
        } = other;

        // Extend existing address / function signature key pairs
        // Add new address / function signature key pairs from the provided EthereumCallFilter
        for (address, (proposed_start_block, new_sigs)) in
            contract_addresses_function_signatures.into_iter()
        {
            match self
                .contract_addresses_function_signatures
                .get_mut(&address)
            {
                Some((existing_start_block, existing_sigs)) => {
                    *existing_start_block = cmp::min(proposed_start_block, *existing_start_block);
                    existing_sigs.extend(new_sigs);
                }
                None => {
                    self.contract_addresses_function_signatures
                        .insert(address, (proposed_start_block, new_sigs));
                }
            }
        }

        self.wildcard_signatures.extend(wildcard_signatures);
    }

    /// An empty filter is one that never matches.
    pub fn is_empty(&self) -> bool {
        // Destructure to make sure we're checking all fields.
        let EthereumCallFilter {
            contract_addresses_function_signatures,
            wildcard_signatures: wildcard_matches,
        } = self;
        contract_addresses_function_signatures.is_empty() && wildcard_matches.is_empty()
    }
}

impl FromIterator<(BlockNumber, Address, FunctionSelector)> for EthereumCallFilter {
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = (BlockNumber, Address, FunctionSelector)>,
    {
        let mut lookup: HashMap<Address, (BlockNumber, HashSet<FunctionSelector>)> = HashMap::new();
        iter.into_iter()
            .for_each(|(start_block, address, function_signature)| {
                lookup
                    .entry(address)
                    .or_insert((start_block, HashSet::default()));
                lookup.get_mut(&address).map(|set| {
                    if set.0 > start_block {
                        set.0 = start_block
                    }
                    set.1.insert(function_signature);
                    set
                });
            });
        EthereumCallFilter {
            contract_addresses_function_signatures: lookup,
            wildcard_signatures: HashSet::new(),
        }
    }
}

impl From<&EthereumBlockFilter> for EthereumCallFilter {
    fn from(ethereum_block_filter: &EthereumBlockFilter) -> Self {
        Self {
            contract_addresses_function_signatures: ethereum_block_filter
                .contract_addresses
                .iter()
                .map(|(start_block_opt, address)| {
                    (*address, (*start_block_opt, HashSet::default()))
                })
                .collect::<HashMap<Address, (BlockNumber, HashSet<FunctionSelector>)>>(),
            wildcard_signatures: HashSet::new(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct EthereumBlockFilter {
    /// Used for polling block handlers, a hashset of (start_block, polling_interval)
    pub polling_intervals: HashSet<(BlockNumber, i32)>,
    pub contract_addresses: HashSet<(BlockNumber, Address)>,
    pub trigger_every_block: bool,
}

impl Into<Vec<CallToFilter>> for EthereumBlockFilter {
    fn into(self) -> Vec<CallToFilter> {
        self.contract_addresses
            .into_iter()
            .map(|(_, addr)| addr)
            .sorted()
            .dedup_by(|x, y| x == y)
            .map(|addr| CallToFilter {
                addresses: vec![addr.to_fixed_bytes().to_vec()],
                signatures: vec![],
            })
            .collect_vec()
    }
}

impl EthereumBlockFilter {
    /// from_mapping ignores contract addresses in this use case because templates can't provide Address or BlockNumber
    /// ahead of time. This means the filters applied to the block_stream need to be broad, in this case,
    /// specifically, will match all blocks. The blocks are then further filtered by the subgraph instance manager
    /// which keeps track of deployed contracts and relevant addresses.
    pub fn from_mapping(mapping: &Mapping) -> Self {
        Self {
            polling_intervals: HashSet::new(),
            contract_addresses: HashSet::new(),
            trigger_every_block: !mapping.block_handlers.is_empty(),
        }
    }

    pub fn from_data_sources<'a>(iter: impl IntoIterator<Item = &'a DataSource>) -> Self {
        iter.into_iter()
            .filter(|data_source| data_source.address.is_some())
            .fold(Self::default(), |mut filter_opt, data_source| {
                let has_block_handler_with_call_filter = data_source
                    .mapping
                    .block_handlers
                    .clone()
                    .into_iter()
                    .any(|block_handler| match block_handler.filter {
                        Some(BlockHandlerFilter::Call) => true,
                        _ => false,
                    });

                let has_block_handler_without_filter = data_source
                    .mapping
                    .block_handlers
                    .clone()
                    .into_iter()
                    .any(|block_handler| block_handler.filter.is_none());

                filter_opt.extend(Self {
                    trigger_every_block: has_block_handler_without_filter,
                    polling_intervals: data_source
                        .mapping
                        .block_handlers
                        .clone()
                        .into_iter()
                        .filter_map(|block_handler| match block_handler.filter {
                            Some(BlockHandlerFilter::Polling { every }) => {
                                Some((data_source.start_block, every.get() as i32))
                            }
                            Some(BlockHandlerFilter::Once) => Some((data_source.start_block, 0)),
                            _ => None,
                        })
                        .collect(),
                    contract_addresses: if has_block_handler_with_call_filter {
                        vec![(data_source.start_block, data_source.address.unwrap())]
                            .into_iter()
                            .collect()
                    } else {
                        HashSet::default()
                    },
                });
                filter_opt
            })
    }

    pub fn extend(&mut self, other: EthereumBlockFilter) {
        if other.is_empty() {
            return;
        };

        let EthereumBlockFilter {
            polling_intervals,
            contract_addresses,
            trigger_every_block,
        } = other;

        self.trigger_every_block = self.trigger_every_block || trigger_every_block;

        for other in contract_addresses {
            let (other_start_block, other_address) = other;

            match self.find_contract_address(&other.1) {
                Some((current_start_block, current_address)) => {
                    if other_start_block < current_start_block {
                        self.contract_addresses
                            .remove(&(current_start_block, current_address));
                        self.contract_addresses
                            .insert((other_start_block, other_address));
                    }
                }
                None => {
                    self.contract_addresses
                        .insert((other_start_block, other_address));
                }
            }
        }

        for (other_start_block, other_polling_interval) in &polling_intervals {
            self.polling_intervals
                .insert((*other_start_block, *other_polling_interval));
        }
    }

    fn requires_traces(&self) -> bool {
        !self.contract_addresses.is_empty()
    }

    /// An empty filter is one that never matches.
    pub fn is_empty(&self) -> bool {
        let Self {
            contract_addresses,
            polling_intervals,
            trigger_every_block,
        } = self;
        // If we are triggering every block, we are of course not empty
        !*trigger_every_block && contract_addresses.is_empty() && polling_intervals.is_empty()
    }

    fn find_contract_address(&self, candidate: &Address) -> Option<(i32, Address)> {
        self.contract_addresses
            .iter()
            .find(|(_, current_address)| candidate == current_address)
            .cloned()
    }
}

pub enum ProviderStatus {
    Working,
    VersionFail,
    GenesisFail,
    VersionTimeout,
    GenesisTimeout,
}

impl From<ProviderStatus> for f64 {
    fn from(state: ProviderStatus) -> Self {
        match state {
            ProviderStatus::Working => 0.0,
            ProviderStatus::VersionFail => 1.0,
            ProviderStatus::GenesisFail => 2.0,
            ProviderStatus::VersionTimeout => 3.0,
            ProviderStatus::GenesisTimeout => 4.0,
        }
    }
}

const STATUS_HELP: &str = "0 = ok, 1 = net_version failed, 2 = get genesis failed, 3 = net_version timeout, 4 = get genesis timeout";
#[derive(Debug, Clone)]
pub struct ProviderEthRpcMetrics {
    request_duration: Box<HistogramVec>,
    errors: Box<CounterVec>,
    status: Box<GaugeVec>,
}

impl ProviderEthRpcMetrics {
    pub fn new(registry: Arc<MetricsRegistry>) -> Self {
        let request_duration = registry
            .new_histogram_vec(
                "eth_rpc_request_duration",
                "Measures eth rpc request duration",
                vec![String::from("method"), String::from("provider")],
                vec![0.05, 0.1, 0.2, 0.4, 0.8, 1.6, 3.2, 6.4, 12.8, 25.6],
            )
            .unwrap();
        let errors = registry
            .new_counter_vec(
                "eth_rpc_errors",
                "Counts eth rpc request errors",
                vec![String::from("method"), String::from("provider")],
            )
            .unwrap();
        let status_help = format!("Whether the provider has failed ({STATUS_HELP})");
        let status = registry
            .new_gauge_vec(
                "eth_rpc_status",
                &status_help,
                vec![String::from("provider")],
            )
            .unwrap();
        Self {
            request_duration,
            errors,
            status,
        }
    }

    pub fn observe_request(&self, duration: f64, method: &str, provider: &str) {
        self.request_duration
            .with_label_values(&[method, provider])
            .observe(duration);
    }

    pub fn add_error(&self, method: &str, provider: &str) {
        self.errors.with_label_values(&[method, provider]).inc();
    }

    pub fn set_status(&self, status: ProviderStatus, provider: &str) {
        self.status
            .with_label_values(&[provider])
            .set(status.into());
    }
}

#[derive(Clone)]
pub struct SubgraphEthRpcMetrics {
    request_duration: GaugeVec,
    errors: CounterVec,
    deployment: String,
}

impl SubgraphEthRpcMetrics {
    pub fn new(registry: Arc<MetricsRegistry>, subgraph_hash: &str) -> Self {
        let request_duration = registry
            .global_gauge_vec(
                "deployment_eth_rpc_request_duration",
                "Measures eth rpc request duration for a subgraph deployment",
                vec!["deployment", "method", "provider"].as_slice(),
            )
            .unwrap();
        let errors = registry
            .global_counter_vec(
                "deployment_eth_rpc_errors",
                "Counts eth rpc request errors for a subgraph deployment",
                vec!["deployment", "method", "provider"].as_slice(),
            )
            .unwrap();
        Self {
            request_duration,
            errors,
            deployment: subgraph_hash.into(),
        }
    }

    pub fn observe_request(&self, duration: f64, method: &str, provider: &str) {
        self.request_duration
            .with_label_values(&[&self.deployment, method, provider])
            .set(duration);
    }

    pub fn add_error(&self, method: &str, provider: &str) {
        self.errors
            .with_label_values(&[&self.deployment, method, provider])
            .inc();
    }
}

/// Common trait for components that watch and manage access to Ethereum.
///
/// Implementations may be implemented against an in-process Ethereum node
/// or a remote node over RPC.
#[async_trait]
pub trait EthereumAdapter: Send + Sync + 'static {
    /// The `provider.label` from the adapter's configuration
    fn provider(&self) -> &str;

    /// Ask the Ethereum node for some identifying information about the Ethereum network it is
    /// connected to.
    async fn net_identifiers(&self) -> Result<ChainIdentifier, Error>;

    /// Get the latest block, including full transactions.
    fn latest_block(
        &self,
        logger: &Logger,
    ) -> Box<dyn Future<Item = LightEthereumBlock, Error = bc::IngestorError> + Send + Unpin>;

    /// Get the latest block, with only the header and transaction hashes.
    fn latest_block_header(
        &self,
        logger: &Logger,
    ) -> Box<dyn Future<Item = web3::types::Block<H256>, Error = bc::IngestorError> + Send>;

    fn load_block(
        &self,
        logger: &Logger,
        block_hash: H256,
    ) -> Box<dyn Future<Item = LightEthereumBlock, Error = Error> + Send>;

    /// Load Ethereum blocks in bulk, returning results as they come back as a Stream.
    /// May use the `chain_store` as a cache.
    fn load_blocks(
        &self,
        logger: Logger,
        chain_store: Arc<dyn ChainStore>,
        block_hashes: HashSet<H256>,
    ) -> Box<dyn Stream<Item = Arc<LightEthereumBlock>, Error = Error> + Send>;

    /// Find a block by its hash.
    fn block_by_hash(
        &self,
        logger: &Logger,
        block_hash: H256,
    ) -> Box<dyn Future<Item = Option<LightEthereumBlock>, Error = Error> + Send>;

    fn block_by_number(
        &self,
        logger: &Logger,
        block_number: BlockNumber,
    ) -> Box<dyn Future<Item = Option<LightEthereumBlock>, Error = Error> + Send>;

    /// Load full information for the specified `block` (in particular, transaction receipts).
    fn load_full_block(
        &self,
        logger: &Logger,
        block: LightEthereumBlock,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<EthereumBlock, bc::IngestorError>> + Send>>;

    /// Load block pointer for the specified `block number`.
    fn block_pointer_from_number(
        &self,
        logger: &Logger,
        block_number: BlockNumber,
    ) -> Box<dyn Future<Item = BlockPtr, Error = bc::IngestorError> + Send>;

    /// Find a block by its number, according to the Ethereum node.
    ///
    /// Careful: don't use this function without considering race conditions.
    /// Chain reorgs could happen at any time, and could affect the answer received.
    /// Generally, it is only safe to use this function with blocks that have received enough
    /// confirmations to guarantee no further reorgs, **and** where the Ethereum node is aware of
    /// those confirmations.
    /// If the Ethereum node is far behind in processing blocks, even old blocks can be subject to
    /// reorgs.
    fn block_hash_by_block_number(
        &self,
        logger: &Logger,
        block_number: BlockNumber,
    ) -> Box<dyn Future<Item = Option<H256>, Error = Error> + Send>;

    /// Call the function of a smart contract.
    fn contract_call(
        &self,
        logger: &Logger,
        call: EthereumContractCall,
        cache: Arc<dyn EthereumCallCache>,
    ) -> Box<dyn Future<Item = Vec<Token>, Error = EthereumContractCallError> + Send>;
}

#[cfg(test)]
mod tests {
    use crate::adapter::{FunctionSelector, COMBINED_FILTER_TYPE_URL};

    use super::{EthereumBlockFilter, LogFilterNode};
    use super::{EthereumCallFilter, EthereumLogFilter, TriggerFilter};

    use graph::blockchain::TriggerFilter as _;
    use graph::firehose::{CallToFilter, CombinedFilter, LogFilter, MultiLogFilter};
    use graph::petgraph::graphmap::GraphMap;
    use graph::prelude::ethabi::ethereum_types::H256;
    use graph::prelude::web3::types::Address;
    use graph::prelude::web3::types::Bytes;
    use graph::prelude::EthereumCall;
    use hex::ToHex;
    use itertools::Itertools;
    use prost::Message;
    use prost_types::Any;

    use std::collections::{HashMap, HashSet};
    use std::iter::FromIterator;
    use std::str::FromStr;

    #[test]
    fn ethereum_log_filter_codec() {
        let hex_addr = "0x4c7b8591c50f4ad308d07d6294f2945e074420f5";
        let address = Address::from_str(hex_addr).expect("unable to parse addr");
        assert_eq!(hex_addr, format!("0x{}", address.encode_hex::<String>()));

        let event_sigs = vec![
            "0xafb42f194014ece77df0f9e4bc3ced9757555dc1fe7dc803161a2de3b7c4839a",
            "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
        ];

        let hex_sigs = event_sigs
            .iter()
            .map(|addr| {
                format!(
                    "0x{}",
                    H256::from_str(addr)
                        .expect("unable to parse addr")
                        .encode_hex::<String>()
                )
            })
            .collect_vec();

        assert_eq!(event_sigs, hex_sigs);

        let sigs = event_sigs
            .iter()
            .map(|addr| {
                H256::from_str(addr)
                    .expect("unable to parse addr")
                    .to_fixed_bytes()
                    .to_vec()
            })
            .collect_vec();

        let filter = LogFilter {
            addresses: vec![address.to_fixed_bytes().to_vec()],
            event_signatures: sigs,
        };
        // This base64 was provided by Streamingfast as a binding example of the expected encoded for the
        // addresses and signatures above.
        let expected_base64 = "CloKFEx7hZHFD0rTCNB9YpTylF4HRCD1EiCvtC8ZQBTs533w+eS8PO2XV1Vdwf59yAMWGi3jt8SDmhIg3fJSrRviyJtpwrBo/DeNqpUrp/FjxKEWKPVaTfUjs+8=";

        let filter = MultiLogFilter {
            log_filters: vec![filter],
        };

        let output = base64::encode(filter.encode_to_vec());
        assert_eq!(expected_base64, output);
    }

    #[test]
    fn ethereum_call_filter_codec() {
        let hex_addr = "0xeed2b7756e295a9300e53dd049aeb0751899bae3";
        let sig = "a9059cbb";
        let mut fs: FunctionSelector = [0u8; 4];
        let hex_sig = hex::decode(sig).expect("failed to parse sig");
        fs.copy_from_slice(&hex_sig[..]);

        let actual_sig = hex::encode(fs);
        assert_eq!(sig, actual_sig);

        let filter = LogFilter {
            addresses: vec![Address::from_str(hex_addr)
                .expect("failed to parse address")
                .to_fixed_bytes()
                .to_vec()],
            event_signatures: vec![fs.to_vec()],
        };

        // This base64 was provided by Streamingfast as a binding example of the expected encoded for the
        // addresses and signatures above.
        let expected_base64 = "ChTu0rd1bilakwDlPdBJrrB1GJm64xIEqQWcuw==";

        let output = base64::encode(filter.encode_to_vec());
        assert_eq!(expected_base64, output);
    }

    #[test]
    fn ethereum_trigger_filter_to_firehose() {
        let address = Address::from_low_u64_be;
        let sig = H256::from_low_u64_le;
        let mut filter = TriggerFilter {
            log: EthereumLogFilter {
                contracts_and_events_graph: GraphMap::new(),
                wildcard_events: HashMap::new(),
            },
            call: EthereumCallFilter {
                contract_addresses_function_signatures: HashMap::from_iter(vec![
                    (address(0), (0, HashSet::from_iter(vec![[0u8; 4]]))),
                    (address(1), (1, HashSet::from_iter(vec![[1u8; 4]]))),
                    (address(2), (2, HashSet::new())),
                ]),
                wildcard_signatures: HashSet::new(),
            },
            block: EthereumBlockFilter {
                polling_intervals: HashSet::from_iter(vec![(1, 10), (3, 24)]),
                contract_addresses: HashSet::from_iter([
                    (100, address(1000)),
                    (200, address(2000)),
                    (300, address(3000)),
                    (400, address(1000)),
                    (500, address(1000)),
                ]),
                trigger_every_block: false,
            },
        };

        let expected_call_filters = vec![
            CallToFilter {
                addresses: vec![address(0).to_fixed_bytes().to_vec()],
                signatures: vec![[0u8; 4].to_vec()],
            },
            CallToFilter {
                addresses: vec![address(1).to_fixed_bytes().to_vec()],
                signatures: vec![[1u8; 4].to_vec()],
            },
            CallToFilter {
                addresses: vec![address(2).to_fixed_bytes().to_vec()],
                signatures: vec![],
            },
            CallToFilter {
                addresses: vec![address(1000).to_fixed_bytes().to_vec()],
                signatures: vec![],
            },
            CallToFilter {
                addresses: vec![address(2000).to_fixed_bytes().to_vec()],
                signatures: vec![],
            },
            CallToFilter {
                addresses: vec![address(3000).to_fixed_bytes().to_vec()],
                signatures: vec![],
            },
        ];

        filter.log.contracts_and_events_graph.add_edge(
            LogFilterNode::Contract(address(10)),
            LogFilterNode::Event(sig(100)),
            false,
        );
        filter.log.contracts_and_events_graph.add_edge(
            LogFilterNode::Contract(address(10)),
            LogFilterNode::Event(sig(101)),
            false,
        );
        filter.log.contracts_and_events_graph.add_edge(
            LogFilterNode::Contract(address(20)),
            LogFilterNode::Event(sig(100)),
            false,
        );

        let expected_log_filters = vec![
            LogFilter {
                addresses: vec![address(10).to_fixed_bytes().to_vec()],
                event_signatures: vec![sig(101).to_fixed_bytes().to_vec()],
            },
            LogFilter {
                addresses: vec![
                    address(10).to_fixed_bytes().to_vec(),
                    address(20).to_fixed_bytes().to_vec(),
                ],
                event_signatures: vec![sig(100).to_fixed_bytes().to_vec()],
            },
        ];

        let firehose_filter = filter.clone().to_firehose_filter();
        assert_eq!(1, firehose_filter.len());

        let firehose_filter: HashMap<_, _> = HashMap::from_iter::<Vec<(String, Any)>>(
            firehose_filter
                .into_iter()
                .map(|any| (any.type_url.clone(), any))
                .collect_vec(),
        );

        let mut combined_filter = &firehose_filter
            .get(COMBINED_FILTER_TYPE_URL)
            .expect("a CombinedFilter")
            .value[..];

        let combined_filter =
            CombinedFilter::decode(&mut combined_filter).expect("combined filter to decode");

        let CombinedFilter {
            log_filters: mut actual_log_filters,
            call_filters: mut actual_call_filters,
            send_all_block_headers: actual_send_all_block_headers,
        } = combined_filter;

        actual_call_filters.sort_by(|a, b| a.addresses.cmp(&b.addresses));
        for filter in actual_call_filters.iter_mut() {
            filter.signatures.sort();
        }
        assert_eq!(expected_call_filters, actual_call_filters);

        actual_log_filters.sort_by(|a, b| a.addresses.cmp(&b.addresses));
        for filter in actual_log_filters.iter_mut() {
            filter.event_signatures.sort();
        }
        assert_eq!(expected_log_filters, actual_log_filters);
        assert_eq!(true, actual_send_all_block_headers);
    }

    #[test]
    fn ethereum_trigger_filter_to_firehose_every_block_plus_logfilter() {
        let address = Address::from_low_u64_be;
        let sig = H256::from_low_u64_le;
        let mut filter = TriggerFilter {
            log: EthereumLogFilter {
                contracts_and_events_graph: GraphMap::new(),
                wildcard_events: HashMap::new(),
            },
            call: EthereumCallFilter {
                contract_addresses_function_signatures: HashMap::new(),
                wildcard_signatures: HashSet::new(),
            },
            block: EthereumBlockFilter {
                polling_intervals: HashSet::default(),
                contract_addresses: HashSet::new(),
                trigger_every_block: true,
            },
        };

        filter.log.contracts_and_events_graph.add_edge(
            LogFilterNode::Contract(address(10)),
            LogFilterNode::Event(sig(101)),
            false,
        );

        let expected_log_filters = vec![LogFilter {
            addresses: vec![address(10).to_fixed_bytes().to_vec()],
            event_signatures: vec![sig(101).to_fixed_bytes().to_vec()],
        }];

        let firehose_filter = filter.clone().to_firehose_filter();
        assert_eq!(1, firehose_filter.len());

        let firehose_filter: HashMap<_, _> = HashMap::from_iter::<Vec<(String, Any)>>(
            firehose_filter
                .into_iter()
                .map(|any| (any.type_url.clone(), any))
                .collect_vec(),
        );

        let mut combined_filter = &firehose_filter
            .get(COMBINED_FILTER_TYPE_URL)
            .expect("a CombinedFilter")
            .value[..];

        let combined_filter =
            CombinedFilter::decode(&mut combined_filter).expect("combined filter to decode");

        let CombinedFilter {
            log_filters: mut actual_log_filters,
            call_filters: actual_call_filters,
            send_all_block_headers: actual_send_all_block_headers,
        } = combined_filter;

        assert_eq!(0, actual_call_filters.len());

        actual_log_filters.sort_by(|a, b| a.addresses.cmp(&b.addresses));
        for filter in actual_log_filters.iter_mut() {
            filter.event_signatures.sort();
        }
        assert_eq!(expected_log_filters, actual_log_filters);

        assert_eq!(true, actual_send_all_block_headers);
    }

    #[test]
    fn matching_ethereum_call_filter() {
        let call = |to: Address, input: Vec<u8>| EthereumCall {
            to,
            input: bytes(input),
            ..Default::default()
        };

        let mut filter = EthereumCallFilter {
            contract_addresses_function_signatures: HashMap::from_iter(vec![
                (address(0), (0, HashSet::from_iter(vec![[0u8; 4]]))),
                (address(1), (1, HashSet::from_iter(vec![[1u8; 4]]))),
                (address(2), (2, HashSet::new())),
            ]),
            wildcard_signatures: HashSet::new(),
        };
        let filter2 = EthereumCallFilter {
            contract_addresses_function_signatures: HashMap::from_iter(vec![(
                address(0),
                (0, HashSet::from_iter(vec![[10u8; 4]])),
            )]),
            wildcard_signatures: HashSet::from_iter(vec![[11u8; 4]]),
        };

        assert_eq!(
            false,
            filter.matches(&call(address(2), vec![])),
            "call with empty bytes are always ignore, whatever the condition"
        );

        assert_eq!(
            false,
            filter.matches(&call(address(4), vec![1; 36])),
            "call with incorrect address should be ignored"
        );

        assert_eq!(
            true,
            filter.matches(&call(address(1), vec![1; 36])),
            "call with correct address & signature should match"
        );

        assert_eq!(
            true,
            filter.matches(&call(address(1), vec![1; 32])),
            "call with correct address & signature, but with incorrect input size should match"
        );

        assert_eq!(
            false,
            filter.matches(&call(address(1), vec![4u8; 36])),
            "call with correct address but incorrect signature for a specific contract filter (i.e. matches some signatures) should be ignored"
        );

        assert_eq!(
            false,
            filter.matches(&call(address(0), vec![11u8; 36])),
            "this signature should not match filter1, this avoid false passes if someone changes the code"
        );
        assert_eq!(
            false,
            filter2.matches(&call(address(1), vec![10u8; 36])),
            "this signature should not match filter2 because the address is not the expected one"
        );
        assert_eq!(
            true,
            filter2.matches(&call(address(0), vec![10u8; 36])),
            "this signature should match filter2 on the non wildcard clause"
        );
        assert_eq!(
            true,
            filter2.matches(&call(address(0), vec![11u8; 36])),
            "this signature should match filter2 on the wildcard clause"
        );

        // extend filter1 and test the filter 2 stuff again
        filter.extend(filter2);
        assert_eq!(
            true,
            filter.matches(&call(address(0), vec![11u8; 36])),
            "this signature should not match filter1, this avoid false passes if someone changes the code"
        );
        assert_eq!(
            false,
            filter.matches(&call(address(1), vec![10u8; 36])),
            "this signature should not match filter2 because the address is not the expected one"
        );
        assert_eq!(
            true,
            filter.matches(&call(address(0), vec![10u8; 36])),
            "this signature should match filter2 on the non wildcard clause"
        );
        assert_eq!(
            true,
            filter.matches(&call(address(0), vec![11u8; 36])),
            "this signature should match filter2 on the wildcard clause"
        );
    }

    #[test]
    fn extending_ethereum_block_filter_no_found() {
        let mut base = EthereumBlockFilter {
            polling_intervals: HashSet::new(),
            contract_addresses: HashSet::new(),
            trigger_every_block: false,
        };

        let extension = EthereumBlockFilter {
            polling_intervals: HashSet::from_iter(vec![(1, 3)]),
            contract_addresses: HashSet::from_iter(vec![(10, address(1))]),
            trigger_every_block: false,
        };

        base.extend(extension);

        assert_eq!(
            HashSet::from_iter(vec![(10, address(1))]),
            base.contract_addresses,
        );

        assert_eq!(HashSet::from_iter(vec![(1, 3)]), base.polling_intervals,);
    }

    #[test]
    fn extending_ethereum_block_filter_conflict_includes_one_copy() {
        let mut base = EthereumBlockFilter {
            polling_intervals: HashSet::from_iter(vec![(3, 3)]),
            contract_addresses: HashSet::from_iter(vec![(10, address(1))]),
            trigger_every_block: false,
        };

        let extension = EthereumBlockFilter {
            polling_intervals: HashSet::from_iter(vec![(2, 3), (3, 3)]),
            contract_addresses: HashSet::from_iter(vec![(2, address(1))]),
            trigger_every_block: false,
        };

        base.extend(extension);

        assert_eq!(
            HashSet::from_iter(vec![(2, address(1))]),
            base.contract_addresses,
        );

        assert_eq!(
            HashSet::from_iter(vec![(2, 3), (3, 3)]),
            base.polling_intervals,
        );
    }

    #[test]
    fn extending_ethereum_block_filter_conflict_doesnt_include_both_copies() {
        let mut base = EthereumBlockFilter {
            polling_intervals: HashSet::from_iter(vec![(2, 3)]),
            contract_addresses: HashSet::from_iter(vec![(2, address(1))]),
            trigger_every_block: false,
        };

        let extension = EthereumBlockFilter {
            polling_intervals: HashSet::from_iter(vec![(3, 3), (2, 3)]),
            contract_addresses: HashSet::from_iter(vec![(10, address(1))]),
            trigger_every_block: false,
        };

        base.extend(extension);

        assert_eq!(
            HashSet::from_iter(vec![(2, address(1))]),
            base.contract_addresses,
        );

        assert_eq!(
            HashSet::from_iter(vec![(2, 3), (3, 3)]),
            base.polling_intervals,
        );
    }

    #[test]
    fn extending_ethereum_block_filter_every_block_in_ext() {
        let mut base = EthereumBlockFilter {
            polling_intervals: HashSet::new(),
            contract_addresses: HashSet::default(),
            trigger_every_block: false,
        };

        let extension = EthereumBlockFilter {
            polling_intervals: HashSet::new(),
            contract_addresses: HashSet::default(),
            trigger_every_block: true,
        };

        base.extend(extension);

        assert_eq!(true, base.trigger_every_block);
    }

    #[test]
    fn extending_ethereum_block_filter_every_block_in_base_and_merge_contract_addresses_and_polling_intervals(
    ) {
        let mut base = EthereumBlockFilter {
            polling_intervals: HashSet::from_iter(vec![(10, 3)]),
            contract_addresses: HashSet::from_iter(vec![(10, address(2))]),
            trigger_every_block: true,
        };

        let extension = EthereumBlockFilter {
            polling_intervals: HashSet::new(),
            contract_addresses: HashSet::from_iter(vec![]),
            trigger_every_block: false,
        };

        base.extend(extension);

        assert_eq!(true, base.trigger_every_block);
        assert_eq!(
            HashSet::from_iter(vec![(10, address(2))]),
            base.contract_addresses,
        );
        assert_eq!(HashSet::from_iter(vec![(10, 3)]), base.polling_intervals,);
    }

    #[test]
    fn extending_ethereum_block_filter_every_block_in_ext_and_merge_contract_addresses() {
        let mut base = EthereumBlockFilter {
            polling_intervals: HashSet::from_iter(vec![(10, 3)]),
            contract_addresses: HashSet::from_iter(vec![(10, address(2))]),
            trigger_every_block: false,
        };

        let extension = EthereumBlockFilter {
            polling_intervals: HashSet::from_iter(vec![(10, 3)]),
            contract_addresses: HashSet::from_iter(vec![(10, address(1))]),
            trigger_every_block: true,
        };

        base.extend(extension);

        assert_eq!(true, base.trigger_every_block);
        assert_eq!(
            HashSet::from_iter(vec![(10, address(2)), (10, address(1))]),
            base.contract_addresses,
        );
        assert_eq!(
            HashSet::from_iter(vec![(10, 3), (10, 3)]),
            base.polling_intervals,
        );
    }

    #[test]
    fn extending_ethereum_call_filter() {
        let mut base = EthereumCallFilter {
            contract_addresses_function_signatures: HashMap::from_iter(vec![
                (
                    Address::from_low_u64_be(0),
                    (0, HashSet::from_iter(vec![[0u8; 4]])),
                ),
                (
                    Address::from_low_u64_be(1),
                    (1, HashSet::from_iter(vec![[1u8; 4]])),
                ),
            ]),
            wildcard_signatures: HashSet::new(),
        };
        let extension = EthereumCallFilter {
            contract_addresses_function_signatures: HashMap::from_iter(vec![
                (
                    Address::from_low_u64_be(0),
                    (2, HashSet::from_iter(vec![[2u8; 4]])),
                ),
                (
                    Address::from_low_u64_be(3),
                    (3, HashSet::from_iter(vec![[3u8; 4]])),
                ),
            ]),
            wildcard_signatures: HashSet::new(),
        };
        base.extend(extension);

        assert_eq!(
            base.contract_addresses_function_signatures
                .get(&Address::from_low_u64_be(0)),
            Some(&(0, HashSet::from_iter(vec![[0u8; 4], [2u8; 4]])))
        );
        assert_eq!(
            base.contract_addresses_function_signatures
                .get(&Address::from_low_u64_be(3)),
            Some(&(3, HashSet::from_iter(vec![[3u8; 4]])))
        );
        assert_eq!(
            base.contract_addresses_function_signatures
                .get(&Address::from_low_u64_be(1)),
            Some(&(1, HashSet::from_iter(vec![[1u8; 4]])))
        );
    }

    fn address(id: u64) -> Address {
        Address::from_low_u64_be(id)
    }

    fn bytes(value: Vec<u8>) -> Bytes {
        Bytes::from(value)
    }
}

// Tests `eth_get_logs_filters` in instances where all events are filtered on by all contracts.
// This represents, for example, the relationship between dynamic data sources and their events.
#[test]
fn complete_log_filter() {
    use std::collections::BTreeSet;

    // Test a few combinations of complete graphs.
    for i in [1, 2] {
        let events: BTreeSet<_> = (0..i).map(H256::from_low_u64_le).collect();

        for j in [1, 1000, 2000, 3000] {
            let contracts: BTreeSet<_> = (0..j).map(Address::from_low_u64_le).collect();

            // Construct the complete bipartite graph with i events and j contracts.
            let mut contracts_and_events_graph = GraphMap::new();
            for &contract in &contracts {
                for &event in &events {
                    contracts_and_events_graph.add_edge(
                        LogFilterNode::Contract(contract),
                        LogFilterNode::Event(event),
                        false,
                    );
                }
            }

            // Run `eth_get_logs_filters`, which is what we want to test.
            let logs_filters: Vec<_> = EthereumLogFilter {
                contracts_and_events_graph,
                wildcard_events: HashMap::new(),
            }
            .eth_get_logs_filters()
            .collect();

            // Assert that a contract or event is filtered on iff it was present in the graph.
            assert_eq!(
                logs_filters
                    .iter()
                    .flat_map(|l| l.contracts.iter())
                    .copied()
                    .collect::<BTreeSet<_>>(),
                contracts
            );
            assert_eq!(
                logs_filters
                    .iter()
                    .flat_map(|l| l.event_signatures.iter())
                    .copied()
                    .collect::<BTreeSet<_>>(),
                events
            );

            // Assert that chunking works.
            for filter in logs_filters {
                assert!(filter.contracts.len() <= ENV_VARS.get_logs_max_contracts);
            }
        }
    }
}

#[test]
fn log_filter_require_transacion_receipt_method() {
    // test data
    let event_signature_a = H256::zero();
    let event_signature_b = H256::from_low_u64_be(1);
    let event_signature_c = H256::from_low_u64_be(2);
    let contract_a = Address::from_low_u64_be(3);
    let contract_b = Address::from_low_u64_be(4);
    let contract_c = Address::from_low_u64_be(5);

    let wildcard_event_with_receipt = H256::from_low_u64_be(6);
    let wildcard_event_without_receipt = H256::from_low_u64_be(7);
    let wildcard_events = [
        (wildcard_event_with_receipt, true),
        (wildcard_event_without_receipt, false),
    ]
    .into_iter()
    .collect();

    let alien_event_signature = H256::from_low_u64_be(8); // those will not be inserted in the graph
    let alien_contract_address = Address::from_low_u64_be(9);

    // test graph nodes
    let event_a_node = LogFilterNode::Event(event_signature_a);
    let event_b_node = LogFilterNode::Event(event_signature_b);
    let event_c_node = LogFilterNode::Event(event_signature_c);
    let contract_a_node = LogFilterNode::Contract(contract_a);
    let contract_b_node = LogFilterNode::Contract(contract_b);
    let contract_c_node = LogFilterNode::Contract(contract_c);

    // build test graph with the following layout:
    //
    // ```dot
    // graph bipartite {
    //
    //     // conected and require a receipt
    //     event_a -- contract_a [ receipt=true  ]
    //     event_b -- contract_b [ receipt=true  ]
    //     event_c -- contract_c [ receipt=true  ]
    //
    //     // connected but don't require a receipt
    //     event_a -- contract_b [ receipt=false ]
    //     event_b -- contract_a [ receipt=false ]
    // }
    // ```
    let mut contracts_and_events_graph = GraphMap::new();

    let event_a_id = contracts_and_events_graph.add_node(event_a_node);
    let event_b_id = contracts_and_events_graph.add_node(event_b_node);
    let event_c_id = contracts_and_events_graph.add_node(event_c_node);
    let contract_a_id = contracts_and_events_graph.add_node(contract_a_node);
    let contract_b_id = contracts_and_events_graph.add_node(contract_b_node);
    let contract_c_id = contracts_and_events_graph.add_node(contract_c_node);
    contracts_and_events_graph.add_edge(event_a_id, contract_a_id, true);
    contracts_and_events_graph.add_edge(event_b_id, contract_b_id, true);
    contracts_and_events_graph.add_edge(event_a_id, contract_b_id, false);
    contracts_and_events_graph.add_edge(event_b_id, contract_a_id, false);
    contracts_and_events_graph.add_edge(event_c_id, contract_c_id, true);

    let filter = EthereumLogFilter {
        contracts_and_events_graph,
        wildcard_events,
    };

    // connected contracts and events graph
    assert!(filter.requires_transaction_receipt(&event_signature_a, Some(&contract_a)));
    assert!(filter.requires_transaction_receipt(&event_signature_b, Some(&contract_b)));
    assert!(filter.requires_transaction_receipt(&event_signature_c, Some(&contract_c)));
    assert!(!filter.requires_transaction_receipt(&event_signature_a, Some(&contract_b)));
    assert!(!filter.requires_transaction_receipt(&event_signature_b, Some(&contract_a)));

    // Event C and Contract C are not connected to the other events and contracts
    assert!(!filter.requires_transaction_receipt(&event_signature_a, Some(&contract_c)));
    assert!(!filter.requires_transaction_receipt(&event_signature_b, Some(&contract_c)));
    assert!(!filter.requires_transaction_receipt(&event_signature_c, Some(&contract_a)));
    assert!(!filter.requires_transaction_receipt(&event_signature_c, Some(&contract_b)));

    // Wildcard events
    assert!(filter.requires_transaction_receipt(&wildcard_event_with_receipt, None));
    assert!(!filter.requires_transaction_receipt(&wildcard_event_without_receipt, None));

    // Alien events and contracts always return false
    assert!(
        !filter.requires_transaction_receipt(&alien_event_signature, Some(&alien_contract_address))
    );
    assert!(!filter.requires_transaction_receipt(&alien_event_signature, None));
    assert!(!filter.requires_transaction_receipt(&alien_event_signature, Some(&contract_a)));
    assert!(!filter.requires_transaction_receipt(&alien_event_signature, Some(&contract_b)));
    assert!(!filter.requires_transaction_receipt(&alien_event_signature, Some(&contract_c)));
    assert!(!filter.requires_transaction_receipt(&event_signature_a, Some(&alien_contract_address)));
    assert!(!filter.requires_transaction_receipt(&event_signature_b, Some(&alien_contract_address)));
    assert!(!filter.requires_transaction_receipt(&event_signature_c, Some(&alien_contract_address)));
}
