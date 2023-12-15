use std::{sync::Arc, time::Instant};

use crate::data_source::MappingABI;
use crate::{
    capabilities::NodeCapabilities, network::EthereumNetworkAdapters, Chain, DataSource,
    EthereumAdapter, EthereumAdapterTrait, EthereumContractCall, EthereumContractCallError,
    ENV_VARS,
};
use anyhow::{Context, Error};
use blockchain::HostFn;
use graph::blockchain::ChainIdentifier;
use graph::components::subgraph::HostMetrics;
use graph::runtime::gas::Gas;
use graph::runtime::{AscIndexId, IndexForAscTypeId};
use graph::{
    blockchain::{self, BlockPtr, HostFnCtx},
    cheap_clone::CheapClone,
    prelude::{
        ethabi::{self, Address, Token},
        EthereumCallCache, Future01CompatExt,
    },
    runtime::{asc_get, asc_new, AscPtr, HostExportError},
    semver::Version,
    slog::{info, trace, Logger},
};
use graph_runtime_wasm::asc_abi::class::{AscEnumArray, EthereumValueKind};

use super::abi::{AscUnresolvedContractCall, AscUnresolvedContractCall_0_0_4};

/// Gas limit for `eth_call`. The value of 50_000_000 is a protocol-wide parameter so this
/// should be changed only for debugging purposes and never on an indexer in the network. This
/// value was chosen because it is the Geth default
/// https://github.com/ethereum/go-ethereum/blob/e4b687cf462870538743b3218906940ae590e7fd/eth/ethconfig/config.go#L91.
/// It is not safe to set something higher because Geth will silently override the gas limit
/// with the default. This means that we do not support indexing against a Geth node with
/// `RPCGasCap` set below 50 million.
// See also f0af4ab0-6b7c-4b68-9141-5b79346a5f61.
const ETH_CALL_GAS: u32 = 50_000_000;

// When making an ethereum call, the maximum ethereum gas is ETH_CALL_GAS which is 50 million. One
// unit of Ethereum gas is at least 100ns according to these benchmarks [1], so 1000 of our gas. In
// the worst case an Ethereum call could therefore consume 50 billion of our gas. However the
// averarge call a subgraph makes is much cheaper or even cached in the call cache. So this cost is
// set to 5 billion gas as a compromise. This allows for 2000 calls per handler with the current
// limits.
//
// [1] - https://www.sciencedirect.com/science/article/abs/pii/S0166531620300900
pub const ETHEREUM_CALL: Gas = Gas::new(5_000_000_000);

pub struct RuntimeAdapter {
    pub eth_adapters: Arc<EthereumNetworkAdapters>,
    pub call_cache: Arc<dyn EthereumCallCache>,
    pub chain_identifier: Arc<ChainIdentifier>,
}

impl blockchain::RuntimeAdapter<Chain> for RuntimeAdapter {
    fn host_fns(&self, ds: &DataSource) -> Result<Vec<HostFn>, Error> {
        let abis = ds.mapping.abis.clone();
        let call_cache = self.call_cache.cheap_clone();
        let eth_adapters = self.eth_adapters.cheap_clone();
        let archive = ds.mapping.requires_archive()?;

        // Check if the current network version is in the eth_call_no_gas list
        let should_skip_gas = ENV_VARS
            .eth_call_no_gas
            .contains(&self.chain_identifier.net_version);

        let eth_call_gas = if should_skip_gas {
            None
        } else {
            Some(ETH_CALL_GAS)
        };

        let ethereum_call = HostFn {
            name: "ethereum.call",
            func: Arc::new(move |ctx, wasm_ptr| {
                // Ethereum calls should prioritise call-only adapters if one is available.
                let eth_adapter = eth_adapters.call_or_cheapest(Some(&NodeCapabilities {
                    archive,
                    traces: false,
                }))?;
                ethereum_call(
                    &eth_adapter,
                    call_cache.cheap_clone(),
                    ctx,
                    wasm_ptr,
                    &abis,
                    eth_call_gas,
                )
                .map(|ptr| ptr.wasm_ptr())
            }),
        };

        Ok(vec![ethereum_call])
    }
}

/// function ethereum.call(call: SmartContractCall): Array<Token> | null
fn ethereum_call(
    eth_adapter: &EthereumAdapter,
    call_cache: Arc<dyn EthereumCallCache>,
    ctx: HostFnCtx<'_>,
    wasm_ptr: u32,
    abis: &[Arc<MappingABI>],
    eth_call_gas: Option<u32>,
) -> Result<AscEnumArray<EthereumValueKind>, HostExportError> {
    ctx.gas
        .consume_host_fn_with_metrics(ETHEREUM_CALL, "ethereum_call")?;

    // For apiVersion >= 0.0.4 the call passed from the mapping includes the
    // function signature; subgraphs using an apiVersion < 0.0.4 don't pass
    // the signature along with the call.
    let call: UnresolvedContractCall = if ctx.heap.api_version() >= Version::new(0, 0, 4) {
        asc_get::<_, AscUnresolvedContractCall_0_0_4, _>(ctx.heap, wasm_ptr.into(), &ctx.gas, 0)?
    } else {
        asc_get::<_, AscUnresolvedContractCall, _>(ctx.heap, wasm_ptr.into(), &ctx.gas, 0)?
    };

    let result = eth_call(
        eth_adapter,
        call_cache,
        &ctx.logger,
        &ctx.block_ptr,
        call,
        abis,
        eth_call_gas,
        ctx.metrics.cheap_clone(),
    )?;
    match result {
        Some(tokens) => Ok(asc_new(ctx.heap, tokens.as_slice(), &ctx.gas)?),
        None => Ok(AscPtr::null()),
    }
}

/// Returns `Ok(None)` if the call was reverted.
fn eth_call(
    eth_adapter: &EthereumAdapter,
    call_cache: Arc<dyn EthereumCallCache>,
    logger: &Logger,
    block_ptr: &BlockPtr,
    unresolved_call: UnresolvedContractCall,
    abis: &[Arc<MappingABI>],
    eth_call_gas: Option<u32>,
    metrics: Arc<HostMetrics>,
) -> Result<Option<Vec<Token>>, HostExportError> {
    let start_time = Instant::now();

    // Obtain the path to the contract ABI
    let contract = abis
        .iter()
        .find(|abi| abi.name == unresolved_call.contract_name)
        .with_context(|| {
            format!(
                "Could not find ABI for contract \"{}\", try adding it to the 'abis' section \
                     of the subgraph manifest",
                unresolved_call.contract_name
            )
        })
        .map_err(HostExportError::Deterministic)?
        .contract
        .clone();

    let function = match unresolved_call.function_signature {
        // Behavior for apiVersion < 0.0.4: look up function by name; for overloaded
        // functions this always picks the same overloaded variant, which is incorrect
        // and may lead to encoding/decoding errors
        None => contract
            .function(unresolved_call.function_name.as_str())
            .with_context(|| {
                format!(
                    "Unknown function \"{}::{}\" called from WASM runtime",
                    unresolved_call.contract_name, unresolved_call.function_name
                )
            })
            .map_err(HostExportError::Deterministic)?,

        // Behavior for apiVersion >= 0.0.04: look up function by signature of
        // the form `functionName(uint256,string) returns (bytes32,string)`; this
        // correctly picks the correct variant of an overloaded function
        Some(ref function_signature) => contract
            .functions_by_name(unresolved_call.function_name.as_str())
            .with_context(|| {
                format!(
                    "Unknown function \"{}::{}\" called from WASM runtime",
                    unresolved_call.contract_name, unresolved_call.function_name
                )
            })
            .map_err(HostExportError::Deterministic)?
            .iter()
            .find(|f| function_signature == &f.signature())
            .with_context(|| {
                format!(
                    "Unknown function \"{}::{}\" with signature `{}` \
                         called from WASM runtime",
                    unresolved_call.contract_name,
                    unresolved_call.function_name,
                    function_signature,
                )
            })
            .map_err(HostExportError::Deterministic)?,
    };

    let call = EthereumContractCall {
        address: unresolved_call.contract_address,
        block_ptr: block_ptr.cheap_clone(),
        function: function.clone(),
        args: unresolved_call.function_args.clone(),
        gas: eth_call_gas,
    };

    // Run Ethereum call in tokio runtime
    let logger1 = logger.clone();
    let call_cache = call_cache.clone();
    let result = match graph::block_on(
            eth_adapter.contract_call(&logger1, call, call_cache).compat()
        ) {
            Ok(tokens) => Ok(Some(tokens)),
            Err(EthereumContractCallError::Revert(reason)) => {
                info!(logger, "Contract call reverted"; "reason" => reason);
                Ok(None)
            }

            // Any error reported by the Ethereum node could be due to the block no longer being on
            // the main chain. This is very unespecific but we don't want to risk failing a
            // subgraph due to a transient error such as a reorg.
            Err(EthereumContractCallError::Web3Error(e)) => Err(HostExportError::PossibleReorg(anyhow::anyhow!(
                "Ethereum node returned an error when calling function \"{}\" of contract \"{}\": {}",
                unresolved_call.function_name,
                unresolved_call.contract_name,
                e
            ))),

            // Also retry on timeouts.
            Err(EthereumContractCallError::Timeout) => Err(HostExportError::PossibleReorg(anyhow::anyhow!(
                "Ethereum node did not respond when calling function \"{}\" of contract \"{}\"",
                unresolved_call.function_name,
                unresolved_call.contract_name,
            ))),

            Err(e) => Err(HostExportError::Unknown(anyhow::anyhow!(
                "Failed to call function \"{}\" of contract \"{}\": {}",
                unresolved_call.function_name,
                unresolved_call.contract_name,
                e
            ))),
        };

    let elapsed = start_time.elapsed();

    metrics.observe_eth_call_execution_time(
        elapsed.as_secs_f64(),
        &unresolved_call.contract_name,
        &unresolved_call.contract_address.to_string(),
        &unresolved_call.function_name,
    );

    trace!(logger, "Contract call finished";
              "address" => &unresolved_call.contract_address.to_string(),
              "contract" => &unresolved_call.contract_name,
              "function" => &unresolved_call.function_name,
              "function_signature" => &unresolved_call.function_signature,
              "time" => format!("{}ms", elapsed.as_millis()));

    result
}

#[derive(Clone, Debug)]
pub struct UnresolvedContractCall {
    pub contract_name: String,
    pub contract_address: Address,
    pub function_name: String,
    pub function_signature: Option<String>,
    pub function_args: Vec<ethabi::Token>,
}

impl AscIndexId for AscUnresolvedContractCall {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::SmartContractCall;
}
