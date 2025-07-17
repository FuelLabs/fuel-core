#[cfg(feature = "subscriptions")]
use crate::client::types::StatusWithTransaction;
use crate::{
    client::{
        schema::{
            Tai64Timestamp,
            TransactionId,
            block::BlockByHeightArgs,
            coins::{
                ExcludeInput,
                SpendQueryElementInput,
            },
            contract::ContractBalanceQueryArgs,
            gas_price::EstimateGasPrice,
            message::MessageStatusArgs,
            relayed_tx::RelayedTransactionStatusArgs,
            tx::{
                DryRunArg,
                TxWithEstimatedPredicatesArg,
            },
        },
        types::{
            RelayedTransactionStatus,
            asset::AssetDetail,
            gas_price::LatestGasPrice,
            message::MessageStatus,
            primitives::{
                Address,
                AssetId,
                BlockId,
                ContractId,
                UtxoId,
            },
            upgrades::StateTransitionBytecode,
        },
    },
    reqwest_ext::{
        FuelGraphQlResponse,
        FuelOperation,
        ReqwestExt,
    },
};
use anyhow::Context;
#[cfg(feature = "subscriptions")]
use base64::prelude::{
    BASE64_STANDARD,
    Engine as _,
};
#[cfg(feature = "subscriptions")]
use cynic::StreamingOperation;
use cynic::{
    Id,
    MutationBuilder,
    Operation,
    QueryBuilder,
};
use fuel_core_types::{
    blockchain::header::{
        ConsensusParametersVersion,
        StateTransitionBytecodeVersion,
    },
    fuel_asm::{
        Instruction,
        Word,
    },
    fuel_tx::{
        BlobId,
        Bytes32,
        ConsensusParameters,
        Receipt,
        Transaction,
        TxId,
    },
    fuel_types::{
        self,
        BlockHeight,
        Nonce,
        canonical::Serialize,
    },
    services::executor::{
        StorageReadReplayEvent,
        TransactionExecutionStatus,
    },
};
#[cfg(feature = "subscriptions")]
use futures::{
    Stream,
    StreamExt,
};
use itertools::Itertools;
use pagination::{
    PageDirection,
    PaginatedResult,
    PaginationRequest,
};
use schema::{
    Bytes,
    ContinueTx,
    ContinueTxArgs,
    ConversionError,
    HexString,
    IdArg,
    MemoryArgs,
    RegisterArgs,
    RunResult,
    SetBreakpoint,
    SetBreakpointArgs,
    SetSingleStepping,
    SetSingleSteppingArgs,
    StartTx,
    StartTxArgs,
    U32,
    U64,
    assets::AssetInfoArg,
    balance::BalanceArgs,
    blob::BlobByIdArgs,
    block::BlockByIdArgs,
    coins::{
        CoinByIdArgs,
        CoinsConnectionArgs,
    },
    contract::{
        ContractBalancesConnectionArgs,
        ContractByIdArgs,
    },
    da_compressed::DaCompressedBlockByHeightArgs,
    gas_price::BlockHorizonArgs,
    storage_read_replay::{
        StorageReadReplay,
        StorageReadReplayArgs,
    },
    tx::{
        AssembleTxArg,
        TransactionsByOwnerConnectionArgs,
        TxArg,
        TxIdArgs,
    },
};
#[cfg(feature = "subscriptions")]
use std::future;
use std::{
    convert::TryInto,
    io::{
        self,
        ErrorKind,
    },
    net,
    str::{
        self,
        FromStr,
    },
    sync::{
        Arc,
        Mutex,
    },
};
use tai64::Tai64;
use tracing as _;
use types::{
    TransactionResponse,
    TransactionStatus,
    assemble_tx::{
        AssembleTransactionResult,
        RequiredBalance,
    },
};

use self::schema::{
    block::ProduceBlockArgs,
    message::{
        MessageProofArgs,
        NonceArgs,
    },
};

pub mod pagination;
pub mod schema;
pub mod types;

type RegisterId = u32;

#[derive(Debug, derive_more::Display, derive_more::From)]
#[non_exhaustive]
/// Error occurring during interaction with the FuelClient
// anyhow::Error is wrapped inside a custom Error type,
// so that we can specific error variants in the future.
pub enum Error {
    /// Unknown or not expected(by architecture) error.
    #[from]
    Other(anyhow::Error),
}

/// Consistency policy for the [`FuelClient`] to define the strategy
/// for the required height feature.
#[derive(Debug)]
pub enum ConsistencyPolicy {
    /// Automatically fetch the next block height from the response and
    /// use it as an input to the next query to guarantee consistency
    /// of the results for the queries.
    Auto {
        /// The required block height for the queries.
        height: Arc<Mutex<Option<BlockHeight>>>,
    },
    /// Use manually sets the block height for all queries
    /// via the [`FuelClient::with_required_fuel_block_height`].
    Manual {
        /// The required block height for the queries.
        height: Option<BlockHeight>,
    },
}

impl Clone for ConsistencyPolicy {
    fn clone(&self) -> Self {
        match self {
            Self::Auto { height } => Self::Auto {
                // We don't want to share the same mutex between the different
                // instances of the `FuelClient`.
                height: Arc::new(Mutex::new(height.lock().ok().and_then(|h| *h))),
            },
            Self::Manual { height } => Self::Manual { height: *height },
        }
    }
}

#[derive(Debug, Default)]
struct ChainStateInfo {
    current_stf_version: Arc<Mutex<Option<StateTransitionBytecodeVersion>>>,
    current_consensus_parameters_version: Arc<Mutex<Option<ConsensusParametersVersion>>>,
}

impl Clone for ChainStateInfo {
    fn clone(&self) -> Self {
        Self {
            current_stf_version: Arc::new(Mutex::new(
                self.current_stf_version.lock().ok().and_then(|v| *v),
            )),
            current_consensus_parameters_version: Arc::new(Mutex::new(
                self.current_consensus_parameters_version
                    .lock()
                    .ok()
                    .and_then(|v| *v),
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FuelClient {
    client: reqwest::Client,
    #[cfg(feature = "subscriptions")]
    cookie: std::sync::Arc<reqwest::cookie::Jar>,
    url: reqwest::Url,
    require_height: ConsistencyPolicy,
    chain_state_info: ChainStateInfo,
}

impl FromStr for FuelClient {
    type Err = anyhow::Error;

    fn from_str(str: &str) -> Result<Self, Self::Err> {
        let mut raw_url = str.to_string();
        if !raw_url.starts_with("http") {
            raw_url = format!("http://{raw_url}");
        }

        let mut url = reqwest::Url::parse(&raw_url)
            .map_err(anyhow::Error::msg)
            .with_context(|| format!("Invalid fuel-core URL: {str}"))?;
        url.set_path("/v1/graphql");

        #[cfg(feature = "subscriptions")]
        {
            let cookie = std::sync::Arc::new(reqwest::cookie::Jar::default());
            let client = reqwest::Client::builder()
                .cookie_provider(cookie.clone())
                .build()
                .map_err(anyhow::Error::msg)?;
            Ok(Self {
                client,
                cookie,
                url,
                require_height: ConsistencyPolicy::Auto {
                    height: Arc::new(Mutex::new(None)),
                },
                chain_state_info: Default::default(),
            })
        }

        #[cfg(not(feature = "subscriptions"))]
        {
            let client = reqwest::Client::new();
            Ok(Self {
                client,
                url,
                require_height: ConsistencyPolicy::Auto {
                    height: Arc::new(Mutex::new(None)),
                },
                chain_state_info: Default::default(),
            })
        }
    }
}

impl<S> From<S> for FuelClient
where
    S: Into<net::SocketAddr>,
{
    fn from(socket: S) -> Self {
        format!("http://{}", socket.into())
            .as_str()
            .parse()
            .unwrap()
    }
}

pub fn from_strings_errors_to_std_error(errors: Vec<String>) -> io::Error {
    let e = errors
        .into_iter()
        .fold(String::from("Response errors"), |mut s, e| {
            s.push_str("; ");
            s.push_str(e.as_str());
            s
        });
    io::Error::new(io::ErrorKind::Other, e)
}

impl FuelClient {
    pub fn new(url: impl AsRef<str>) -> anyhow::Result<Self> {
        Self::from_str(url.as_ref())
    }

    pub fn with_required_fuel_block_height(
        &mut self,
        new_height: Option<BlockHeight>,
    ) -> &mut Self {
        match &mut self.require_height {
            ConsistencyPolicy::Auto { height } => {
                *height.lock().expect("Mutex poisoned") = new_height;
            }
            ConsistencyPolicy::Manual { height } => {
                *height = new_height;
            }
        }
        self
    }

    pub fn use_manual_consistency_policy(
        &mut self,
        height: Option<BlockHeight>,
    ) -> &mut Self {
        self.require_height = ConsistencyPolicy::Manual { height };
        self
    }

    pub fn required_block_height(&self) -> Option<BlockHeight> {
        match &self.require_height {
            ConsistencyPolicy::Auto { height } => height.lock().ok().and_then(|h| *h),
            ConsistencyPolicy::Manual { height } => *height,
        }
    }

    fn update_chain_state_info<R, E>(&self, response: &FuelGraphQlResponse<R, E>) {
        if let Some(current_sft_version) = response
            .extensions
            .as_ref()
            .and_then(|e| e.current_stf_version)
        {
            if let Ok(mut c) = self.chain_state_info.current_stf_version.lock() {
                *c = Some(current_sft_version);
            }
        }

        if let Some(current_consensus_parameters_version) = response
            .extensions
            .as_ref()
            .and_then(|e| e.current_consensus_parameters_version)
        {
            if let Ok(mut c) = self
                .chain_state_info
                .current_consensus_parameters_version
                .lock()
            {
                *c = Some(current_consensus_parameters_version);
            }
        }

        let inner_required_height = match &self.require_height {
            ConsistencyPolicy::Auto { height } => Some(height.clone()),
            ConsistencyPolicy::Manual { .. } => None,
        };

        if let Some(inner_required_height) = inner_required_height {
            if let Some(current_fuel_block_height) = response
                .extensions
                .as_ref()
                .and_then(|e| e.current_fuel_block_height)
            {
                let mut lock = inner_required_height.lock().expect("Mutex poisoned");

                if current_fuel_block_height >= lock.unwrap_or_default() {
                    *lock = Some(current_fuel_block_height);
                }
            }
        }
    }

    /// Send the GraphQL query to the client.
    pub async fn query<ResponseData, Vars>(
        &self,
        q: Operation<ResponseData, Vars>,
    ) -> io::Result<ResponseData>
    where
        Vars: serde::Serialize,
        ResponseData: serde::de::DeserializeOwned + 'static,
    {
        let required_fuel_block_height = self.required_block_height();
        let fuel_operation = FuelOperation::new(q, required_fuel_block_height);
        let response = self
            .client
            .post(self.url.clone())
            .run_fuel_graphql(fuel_operation)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        self.decode_response(response)
    }

    fn decode_response<R, E>(&self, response: FuelGraphQlResponse<R, E>) -> io::Result<R>
    where
        R: serde::de::DeserializeOwned + 'static,
    {
        self.update_chain_state_info(&response);

        if let Some(failed) = response
            .extensions
            .as_ref()
            .and_then(|e| e.fuel_block_height_precondition_failed)
        {
            if failed {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "The required block height was not met",
                ));
            }
        }

        let response = response.response;

        match (response.data, response.errors) {
            (Some(d), _) => Ok(d),
            (_, Some(e)) => Err(from_strings_errors_to_std_error(
                e.into_iter().map(|e| e.message).collect(),
            )),
            _ => Err(io::Error::new(io::ErrorKind::Other, "Invalid response")),
        }
    }

    #[tracing::instrument(skip_all)]
    #[cfg(feature = "subscriptions")]
    async fn subscribe<ResponseData, Vars>(
        &self,
        q: StreamingOperation<ResponseData, Vars>,
    ) -> io::Result<impl futures::Stream<Item = io::Result<ResponseData>> + '_>
    where
        Vars: serde::Serialize,
        ResponseData: serde::de::DeserializeOwned + 'static,
    {
        use core::ops::Deref;
        use eventsource_client as es;
        use hyper_rustls as _;
        use reqwest::cookie::CookieStore;
        let mut url = self.url.clone();
        url.set_path("/v1/graphql-sub");

        let required_fuel_block_height = self.required_block_height();
        let fuel_operation = FuelOperation::new(q, required_fuel_block_height);

        let json_query = serde_json::to_string(&fuel_operation)?;
        let mut client_builder = es::ClientBuilder::for_url(url.as_str())
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to start client {e:?}"),
                )
            })?
            .body(json_query)
            .method("POST".to_string())
            .header("content-type", "application/json")
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to add header to client {e:?}"),
                )
            })?;
        if let Some(password) = url.password() {
            let username = url.username();
            let credentials = format!("{}:{}", username, password);
            let authorization = format!("Basic {}", BASE64_STANDARD.encode(credentials));
            client_builder = client_builder
                .header("Authorization", &authorization)
                .map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("Failed to add header to client {e:?}"),
                    )
                })?;
        }

        if let Some(value) = self.cookie.deref().cookies(&self.url) {
            let value = value.to_str().map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Unable convert header value to string {e:?}"),
                )
            })?;
            client_builder = client_builder
                .header(reqwest::header::COOKIE.as_str(), value)
                .map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("Failed to add header from `reqwest` to client {e:?}"),
                    )
                })?;
        }

        let client = client_builder.build_with_conn(
            hyper_rustls::HttpsConnectorBuilder::new()
                .with_webpki_roots()
                .https_or_http()
                .enable_http1()
                .build(),
        );

        let mut last = None;

        let stream = es::Client::stream(&client)
            .take_while(|result| {
                futures::future::ready(!matches!(result, Err(es::Error::Eof)))
            })
            .filter_map(move |result| {
                tracing::debug!("Got result: {result:?}");
                let r = match result {
                    Ok(es::SSE::Event(es::Event { data, .. })) => {
                        match serde_json::from_str::<FuelGraphQlResponse<ResponseData>>(
                            &data,
                        ) {
                            Ok(resp) => {
                                match self.decode_response(resp) {
                                    Ok(resp) => {
                                        match last.replace(data) {
                                            // Remove duplicates
                                            Some(l)
                                                if l == *last.as_ref().expect(
                                                    "Safe because of the replace above",
                                                ) =>
                                            {
                                                None
                                            }
                                            _ => Some(Ok(resp)),
                                        }
                                    }
                                    Err(e) => Some(Err(io::Error::new(
                                        io::ErrorKind::Other,
                                        format!("Decode error: {e:?}"),
                                    ))),
                                }
                            }
                            Err(e) => Some(Err(io::Error::new(
                                io::ErrorKind::Other,
                                format!("Json error: {e:?}"),
                            ))),
                        }
                    }
                    Ok(_) => None,
                    Err(e) => Some(Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("Graphql error: {e:?}"),
                    ))),
                };
                futures::future::ready(r)
            });

        Ok(stream)
    }

    pub fn latest_stf_version(&self) -> Option<StateTransitionBytecodeVersion> {
        self.chain_state_info
            .current_stf_version
            .lock()
            .ok()
            .and_then(|value| *value)
    }

    pub fn latest_consensus_parameters_version(
        &self,
    ) -> Option<ConsensusParametersVersion> {
        self.chain_state_info
            .current_consensus_parameters_version
            .lock()
            .ok()
            .and_then(|value| *value)
    }

    pub async fn health(&self) -> io::Result<bool> {
        let query = schema::Health::build(());
        self.query(query).await.map(|r| r.health)
    }

    pub async fn node_info(&self) -> io::Result<types::NodeInfo> {
        let query = schema::node_info::QueryNodeInfo::build(());
        self.query(query).await.map(|r| r.node_info.into())
    }

    pub async fn latest_gas_price(&self) -> io::Result<LatestGasPrice> {
        let query = schema::gas_price::QueryLatestGasPrice::build(());
        self.query(query).await.map(|r| r.latest_gas_price.into())
    }

    pub async fn estimate_gas_price(
        &self,
        block_horizon: u32,
    ) -> io::Result<EstimateGasPrice> {
        let args = BlockHorizonArgs {
            block_horizon: Some(block_horizon.into()),
        };
        let query = schema::gas_price::QueryEstimateGasPrice::build(args);
        self.query(query).await.map(|r| r.estimate_gas_price)
    }

    #[cfg(feature = "std")]
    pub async fn connected_peers_info(
        &self,
    ) -> io::Result<Vec<fuel_core_types::services::p2p::PeerInfo>> {
        let query = schema::node_info::QueryPeersInfo::build(());
        self.query(query)
            .await
            .map(|r| r.node_info.peers.into_iter().map(Into::into).collect())
    }

    pub async fn chain_info(&self) -> io::Result<types::ChainInfo> {
        let query = schema::chain::ChainQuery::build(());
        self.query(query).await.and_then(|r| {
            let result = r.chain.try_into()?;
            Ok(result)
        })
    }

    pub async fn consensus_parameters(
        &self,
        version: i32,
    ) -> io::Result<Option<ConsensusParameters>> {
        let args = schema::upgrades::ConsensusParametersByVersionArgs { version };
        let query = schema::upgrades::ConsensusParametersByVersionQuery::build(args);

        let result = self
            .query(query)
            .await?
            .consensus_parameters
            .map(TryInto::try_into)
            .transpose()?;

        Ok(result)
    }

    pub async fn state_transition_byte_code_by_version(
        &self,
        version: i32,
    ) -> io::Result<Option<StateTransitionBytecode>> {
        let args = schema::upgrades::StateTransitionBytecodeByVersionArgs { version };
        let query = schema::upgrades::StateTransitionBytecodeByVersionQuery::build(args);

        let result = self
            .query(query)
            .await?
            .state_transition_bytecode_by_version
            .map(TryInto::try_into)
            .transpose()?;

        Ok(result)
    }

    pub async fn state_transition_byte_code_by_root(
        &self,
        root: Bytes32,
    ) -> io::Result<Option<StateTransitionBytecode>> {
        let args = schema::upgrades::StateTransitionBytecodeByRootArgs {
            root: HexString(Bytes(root.to_vec())),
        };
        let query = schema::upgrades::StateTransitionBytecodeByRootQuery::build(args);

        let result = self
            .query(query)
            .await?
            .state_transition_bytecode_by_root
            .map(TryInto::try_into)
            .transpose()?;

        Ok(result)
    }

    /// Default dry run, matching the exact configuration as the node
    pub async fn dry_run(
        &self,
        txs: &[Transaction],
    ) -> io::Result<Vec<TransactionExecutionStatus>> {
        self.dry_run_opt(txs, None, None, None).await
    }

    /// Dry run with options to override the node behavior
    pub async fn dry_run_opt(
        &self,
        txs: &[Transaction],
        // Disable utxo input checks (exists, unspent, and valid signature)
        utxo_validation: Option<bool>,
        gas_price: Option<u64>,
        at_height: Option<BlockHeight>,
    ) -> io::Result<Vec<TransactionExecutionStatus>> {
        let txs = txs
            .iter()
            .map(|tx| HexString(Bytes(tx.to_bytes())))
            .collect::<Vec<HexString>>();
        let query: Operation<schema::tx::DryRun, DryRunArg> =
            schema::tx::DryRun::build(DryRunArg {
                txs,
                utxo_validation,
                gas_price: gas_price.map(|gp| gp.into()),
                block_height: at_height.map(|bh| bh.into()),
            });
        let tx_statuses = self.query(query).await.map(|r| r.dry_run)?;
        tx_statuses
            .into_iter()
            .map(|tx_status| tx_status.try_into().map_err(Into::into))
            .collect()
    }

    /// Like `dry_run_opt`, but also returns the storage reads
    pub async fn dry_run_opt_record_storage_reads(
        &self,
        txs: &[Transaction],
        // Disable utxo input checks (exists, unspent, and valid signature)
        utxo_validation: Option<bool>,
        gas_price: Option<u64>,
        at_height: Option<BlockHeight>,
    ) -> io::Result<(Vec<TransactionExecutionStatus>, Vec<StorageReadReplayEvent>)> {
        let txs = txs
            .iter()
            .map(|tx| HexString(Bytes(tx.to_bytes())))
            .collect::<Vec<HexString>>();
        let query: Operation<schema::tx::DryRunRecordStorageReads, DryRunArg> =
            schema::tx::DryRunRecordStorageReads::build(DryRunArg {
                txs,
                utxo_validation,
                gas_price: gas_price.map(|gp| gp.into()),
                block_height: at_height.map(|bh| bh.into()),
            });
        let result = self
            .query(query)
            .await
            .map(|r| r.dry_run_record_storage_reads)?;
        let tx_statuses = result
            .tx_statuses
            .into_iter()
            .map(|tx_status| tx_status.try_into().map_err(Into::into))
            .collect::<io::Result<Vec<_>>>()?;
        let storage_reads = result
            .storage_reads
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>();
        Ok((tx_statuses, storage_reads))
    }

    /// Get storage read replay for a block
    pub async fn storage_read_replay(
        &self,
        height: &BlockHeight,
    ) -> io::Result<Vec<StorageReadReplayEvent>> {
        let query: Operation<StorageReadReplay, StorageReadReplayArgs> =
            StorageReadReplay::build(StorageReadReplayArgs {
                height: (*height).into(),
            });
        Ok(self
            .query(query)
            .await
            .map(|r| r.storage_read_replay)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// Assembles the transaction based on the provided requirements.
    /// The return transaction contains:
    /// - Input coins to cover `required_balances`
    /// - Input coins to cover the fee of the transaction based on the gas price from `block_horizon`
    /// - `Change` or `Destroy` outputs for all assets from the inputs
    /// - `Variable` outputs in the case they are required during the execution
    /// - `Contract` inputs and outputs in the case they are required during the execution
    /// - Reserved witness slots for signed coins filled with `64` zeroes
    /// - Set script gas limit(unless `script` is empty)
    /// - Estimated predicates, if `estimate_predicates == true`
    ///
    /// Returns an error if:
    /// - The number of required balances exceeds the maximum number of inputs allowed.
    /// - The fee address index is out of bounds.
    /// - The same asset has multiple change policies(either the receiver of
    ///   the change is different, or one of the policies states about the destruction
    ///   of the token while the other does not). The `Change` output from the transaction
    ///   also count as a `ChangePolicy`.
    /// - The number of excluded coin IDs exceeds the maximum number of inputs allowed.
    /// - Required assets have multiple entries.
    /// - If accounts don't have sufficient amounts to cover the transaction requirements in assets.
    /// - If a constructed transaction breaks the rules defined by consensus parameters.
    #[allow(clippy::too_many_arguments)]
    pub async fn assemble_tx(
        &self,
        tx: &Transaction,
        block_horizon: u32,
        required_balances: Vec<RequiredBalance>,
        fee_address_index: u16,
        exclude: Option<(Vec<UtxoId>, Vec<Nonce>)>,
        estimate_predicates: bool,
        reserve_gas: Option<u64>,
    ) -> io::Result<AssembleTransactionResult> {
        let tx = HexString(Bytes(tx.to_bytes()));
        let block_horizon = block_horizon.into();

        let required_balances: Vec<_> = required_balances
            .into_iter()
            .map(schema::tx::RequiredBalance::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        let fee_address_index = fee_address_index.into();

        let exclude_input = exclude.map(Into::into);

        let reserve_gas = reserve_gas.map(U64::from);

        let query_arg = AssembleTxArg {
            tx,
            block_horizon,
            required_balances,
            fee_address_index,
            exclude_input,
            estimate_predicates,
            reserve_gas,
        };

        let query = schema::tx::AssembleTx::build(query_arg);
        let assemble_tx_result = self.query(query).await.map(|r| r.assemble_tx)?;
        Ok(assemble_tx_result.try_into()?)
    }

    /// Estimate predicates for the transaction
    pub async fn estimate_predicates(&self, tx: &mut Transaction) -> io::Result<()> {
        let serialized_tx = tx.to_bytes();
        let query = schema::tx::EstimatePredicates::build(TxArg {
            tx: HexString(Bytes(serialized_tx)),
        });
        let tx_with_predicate = self.query(query).await.map(|r| r.estimate_predicates)?;
        let tx_with_predicate: Transaction = tx_with_predicate.try_into()?;
        *tx = tx_with_predicate;
        Ok(())
    }

    pub async fn submit(
        &self,
        tx: &Transaction,
    ) -> io::Result<types::primitives::TransactionId> {
        self.submit_opt(tx, None).await
    }

    pub async fn submit_opt(
        &self,
        tx: &Transaction,
        estimate_predicates: Option<bool>,
    ) -> io::Result<types::primitives::TransactionId> {
        let tx = tx.clone().to_bytes();
        let query = schema::tx::Submit::build(TxWithEstimatedPredicatesArg {
            tx: HexString(Bytes(tx)),
            estimate_predicates,
        });

        let id = self.query(query).await.map(|r| r.submit)?.id.into();
        Ok(id)
    }

    /// Similar to [`Self::submit_and_await_commit_opt`], but with default options.
    #[cfg(feature = "subscriptions")]
    pub async fn submit_and_await_commit(
        &self,
        tx: &Transaction,
    ) -> io::Result<TransactionStatus> {
        self.submit_and_await_commit_opt(tx, None).await
    }

    /// Submit the transaction and wait for it either to be included in
    /// a block or removed from `TxPool`.
    ///
    /// If `estimate_predicates` is set, the predicates will be estimated before
    /// the transaction is inserted into transaction pool.
    ///
    /// This will wait forever if needed, so consider wrapping this call
    /// with a `tokio::time::timeout`.
    #[cfg(feature = "subscriptions")]
    pub async fn submit_and_await_commit_opt(
        &self,
        tx: &Transaction,
        estimate_predicates: Option<bool>,
    ) -> io::Result<TransactionStatus> {
        use cynic::SubscriptionBuilder;
        let tx = tx.clone().to_bytes();
        let s =
            schema::tx::SubmitAndAwaitSubscription::build(TxWithEstimatedPredicatesArg {
                tx: HexString(Bytes(tx)),
                estimate_predicates,
            });

        let mut stream = self.subscribe(s).await?.map(
            |r: io::Result<schema::tx::SubmitAndAwaitSubscription>| {
                let status: TransactionStatus = r?.submit_and_await.try_into()?;
                Result::<_, io::Error>::Ok(status)
            },
        );

        let status = stream.next().await.ok_or(io::Error::new(
            io::ErrorKind::Other,
            "Failed to get status from the submission",
        ))??;

        Ok(status)
    }

    /// Similar to [`Self::submit_and_await_commit`], but the status also contains transaction.
    #[cfg(feature = "subscriptions")]
    pub async fn submit_and_await_commit_with_tx(
        &self,
        tx: &Transaction,
    ) -> io::Result<StatusWithTransaction> {
        self.submit_and_await_commit_with_tx_opt(tx, None).await
    }

    /// Similar to [`Self::submit_and_await_commit_opt`], but the status also contains transaction.
    #[cfg(feature = "subscriptions")]
    pub async fn submit_and_await_commit_with_tx_opt(
        &self,
        tx: &Transaction,
        estimate_predicates: Option<bool>,
    ) -> io::Result<StatusWithTransaction> {
        use cynic::SubscriptionBuilder;
        let tx = tx.clone().to_bytes();
        let s = schema::tx::SubmitAndAwaitSubscriptionWithTransaction::build(
            TxWithEstimatedPredicatesArg {
                tx: HexString(Bytes(tx)),
                estimate_predicates,
            },
        );

        let mut stream = self.subscribe(s).await?.map(
            |r: io::Result<schema::tx::SubmitAndAwaitSubscriptionWithTransaction>| {
                let status: StatusWithTransaction = r?.submit_and_await.try_into()?;
                Result::<_, io::Error>::Ok(status)
            },
        );

        let status = stream.next().await.ok_or(io::Error::new(
            io::ErrorKind::Other,
            "Failed to get status from the submission",
        ))??;

        Ok(status)
    }

    /// Similar to [`Self::submit_and_await_commit`], but includes all intermediate states.
    #[cfg(feature = "subscriptions")]
    pub async fn submit_and_await_status(
        &self,
        tx: &Transaction,
    ) -> io::Result<impl Stream<Item = io::Result<TransactionStatus>> + '_> {
        self.submit_and_await_status_opt(tx, None, None).await
    }

    /// Similar to [`Self::submit_and_await_commit_opt`], but includes all intermediate states.
    #[cfg(feature = "subscriptions")]
    pub async fn submit_and_await_status_opt(
        &self,
        tx: &Transaction,
        estimate_predicates: Option<bool>,
        include_preconfirmation: Option<bool>,
    ) -> io::Result<impl Stream<Item = io::Result<TransactionStatus>> + '_> {
        use cynic::SubscriptionBuilder;
        use schema::tx::SubmitAndAwaitStatusArg;
        let tx = tx.clone().to_bytes();
        let s = schema::tx::SubmitAndAwaitStatusSubscription::build(
            SubmitAndAwaitStatusArg {
                tx: HexString(Bytes(tx)),
                estimate_predicates,
                include_preconfirmation,
            },
        );

        let stream = self.subscribe(s).await?.map(
            |r: io::Result<schema::tx::SubmitAndAwaitStatusSubscription>| {
                let status: TransactionStatus = r?.submit_and_await_status.try_into()?;
                Result::<_, io::Error>::Ok(status)
            },
        );

        Ok(stream)
    }

    /// Requests all storage slots for the `contract_id`.
    #[cfg(feature = "subscriptions")]
    pub async fn contract_storage_slots(
        &self,
        contract_id: &ContractId,
    ) -> io::Result<impl Stream<Item = io::Result<(Bytes32, Vec<u8>)>> + '_> {
        use cynic::SubscriptionBuilder;
        use schema::storage::ContractStorageSlotsArgs;
        let s = schema::storage::ContractStorageSlots::build(ContractStorageSlotsArgs {
            contract_id: (*contract_id).into(),
        });

        let stream = self.subscribe(s).await?.map(
            |result: io::Result<schema::storage::ContractStorageSlots>| {
                let result: (Bytes32, Vec<u8>) = result?.contract_storage_slots.into();
                Result::<_, io::Error>::Ok(result)
            },
        );

        Ok(stream)
    }

    /// Requests all storage balances for the `contract_id`.
    #[cfg(feature = "subscriptions")]
    pub async fn contract_storage_balances(
        &self,
        contract_id: &ContractId,
    ) -> io::Result<impl Stream<Item = io::Result<schema::contract::ContractBalance>> + '_>
    {
        use cynic::SubscriptionBuilder;
        use schema::{
            contract::ContractBalance,
            storage::ContractStorageBalancesArgs,
        };
        let s = schema::storage::ContractStorageBalances::build(
            ContractStorageBalancesArgs {
                contract_id: (*contract_id).into(),
            },
        );

        let stream = self.subscribe(s).await?.map(
            |result: io::Result<schema::storage::ContractStorageBalances>| {
                let result: ContractBalance = result?.contract_storage_balances;
                Result::<_, io::Error>::Ok(result)
            },
        );

        Ok(stream)
    }

    pub async fn contract_slots_values(
        &self,
        contract_id: &ContractId,
        block_height: Option<BlockHeight>,
        requested_storage_slots: Vec<Bytes32>,
    ) -> io::Result<Vec<(Bytes32, Vec<u8>)>> {
        let query = schema::storage::ContractSlotValues::build(
            schema::storage::ContractSlotValuesArgs {
                contract_id: (*contract_id).into(),
                block_height: block_height.map(|b| (*b).into()),
                storage_slots: requested_storage_slots
                    .into_iter()
                    .map(Into::into)
                    .collect(),
            },
        );

        self.query(query)
            .await
            .map(|r| r.contract_slot_values.into_iter().map(Into::into).collect())
    }

    pub async fn contract_balance_values(
        &self,
        contract_id: &ContractId,
        block_height: Option<BlockHeight>,
        requested_storage_slots: Vec<AssetId>,
    ) -> io::Result<Vec<schema::contract::ContractBalance>> {
        let query = schema::storage::ContractBalanceValues::build(
            schema::storage::ContractBalanceValuesArgs {
                contract_id: (*contract_id).into(),
                block_height: block_height.map(|b| (*b).into()),
                assets: requested_storage_slots
                    .into_iter()
                    .map(Into::into)
                    .collect(),
            },
        );

        self.query(query)
            .await
            .map(|r| r.contract_balance_values.into_iter().collect())
    }

    pub async fn start_session(&self) -> io::Result<String> {
        let query = schema::StartSession::build(());

        self.query(query)
            .await
            .map(|r| r.start_session.into_inner())
    }

    pub async fn end_session(&self, id: &str) -> io::Result<bool> {
        let query = schema::EndSession::build(IdArg { id: id.into() });

        self.query(query).await.map(|r| r.end_session)
    }

    pub async fn reset(&self, id: &str) -> io::Result<bool> {
        let query = schema::Reset::build(IdArg { id: id.into() });

        self.query(query).await.map(|r| r.reset)
    }

    pub async fn execute(&self, id: &str, op: &Instruction) -> io::Result<bool> {
        let op = serde_json::to_string(op)?;
        let query = schema::Execute::build(schema::ExecuteArgs { id: id.into(), op });

        self.query(query).await.map(|r| r.execute)
    }

    pub async fn register(&self, id: &str, register: RegisterId) -> io::Result<Word> {
        let query = schema::Register::build(RegisterArgs {
            id: id.into(),
            register: register.into(),
        });

        Ok(self.query(query).await?.register.0 as Word)
    }

    pub async fn memory(&self, id: &str, start: u32, size: u32) -> io::Result<Vec<u8>> {
        let query = schema::Memory::build(MemoryArgs {
            id: id.into(),
            start: start.into(),
            size: size.into(),
        });

        let memory = self.query(query).await?.memory;

        Ok(serde_json::from_str(memory.as_str())?)
    }

    pub async fn set_breakpoint(
        &self,
        session_id: &str,
        contract: fuel_types::ContractId,
        pc: u64,
    ) -> io::Result<()> {
        let operation = SetBreakpoint::build(SetBreakpointArgs {
            id: Id::new(session_id),
            bp: schema::Breakpoint {
                contract: contract.into(),
                pc: U64(pc),
            },
        });

        let response = self.query(operation).await?;
        assert!(
            response.set_breakpoint,
            "Setting breakpoint returned invalid reply"
        );
        Ok(())
    }

    pub async fn set_single_stepping(
        &self,
        session_id: &str,
        enable: bool,
    ) -> io::Result<()> {
        let operation = SetSingleStepping::build(SetSingleSteppingArgs {
            id: Id::new(session_id),
            enable,
        });
        self.query(operation).await?;
        Ok(())
    }

    pub async fn start_tx(
        &self,
        session_id: &str,
        tx: &Transaction,
    ) -> io::Result<RunResult> {
        let operation = StartTx::build(StartTxArgs {
            id: Id::new(session_id),
            tx: serde_json::to_string(tx).expect("Couldn't serialize tx to json"),
        });
        let response = self.query(operation).await?.start_tx;
        Ok(response)
    }

    pub async fn continue_tx(&self, session_id: &str) -> io::Result<RunResult> {
        let operation = ContinueTx::build(ContinueTxArgs {
            id: Id::new(session_id),
        });
        let response = self.query(operation).await?.continue_tx;
        Ok(response)
    }

    pub async fn transaction(
        &self,
        id: &TxId,
    ) -> io::Result<Option<TransactionResponse>> {
        let query = schema::tx::TransactionQuery::build(TxIdArgs { id: (*id).into() });

        let transaction = self.query(query).await?.transaction;

        Ok(transaction.map(|tx| tx.try_into()).transpose()?)
    }

    /// Get the status of a transaction
    pub async fn transaction_status(&self, id: &TxId) -> io::Result<TransactionStatus> {
        let query =
            schema::tx::TransactionStatusQuery::build(TxIdArgs { id: (*id).into() });

        let status = self.query(query).await?.transaction.ok_or_else(|| {
            io::Error::new(
                ErrorKind::NotFound,
                format!("status not found for transaction {id} "),
            )
        })?;

        let status = status
            .status
            .ok_or_else(|| {
                io::Error::new(
                    ErrorKind::NotFound,
                    format!("status not found for transaction {id}"),
                )
            })?
            .try_into()?;
        Ok(status)
    }

    #[tracing::instrument(skip(self), level = "debug")]
    #[cfg(feature = "subscriptions")]
    /// Similar to [`Self::subscribe_transaction_status_opt`], but with default options.
    pub async fn subscribe_transaction_status(
        &self,
        id: &TxId,
    ) -> io::Result<impl futures::Stream<Item = io::Result<TransactionStatus>> + '_> {
        self.subscribe_transaction_status_opt(id, None).await
    }

    #[cfg(feature = "subscriptions")]
    /// Subscribe to the status of a transaction
    pub async fn subscribe_transaction_status_opt(
        &self,
        id: &TxId,
        include_preconfirmation: Option<bool>,
    ) -> io::Result<impl Stream<Item = io::Result<TransactionStatus>> + '_> {
        use cynic::SubscriptionBuilder;
        use schema::tx::StatusChangeSubscriptionArgs;
        let tx_id: TransactionId = (*id).into();
        let s =
            schema::tx::StatusChangeSubscription::build(StatusChangeSubscriptionArgs {
                id: tx_id,
                include_preconfirmation,
            });

        tracing::debug!("subscribing");
        let stream = self.subscribe(s).await?.map(|tx| {
            tracing::debug!("received {tx:?}");
            let tx = tx?;
            let status = tx.status_change.try_into()?;
            Ok(status)
        });

        Ok(stream)
    }

    #[cfg(feature = "subscriptions")]
    /// Awaits for the transaction to be committed into a block
    ///
    /// This will wait forever if needed, so consider wrapping this call
    /// with a `tokio::time::timeout`.
    pub async fn await_transaction_commit(
        &self,
        id: &TxId,
    ) -> io::Result<TransactionStatus> {
        // skip until we've reached a final status and then stop consuming the stream
        // to avoid an EOF which the eventsource client considers as an error.
        let status_result = self
            .subscribe_transaction_status(id)
            .await?
            .skip_while(|status| {
                future::ready(status.as_ref().map_or(true, |status| !status.is_final()))
            })
            .next()
            .await;

        if let Some(Ok(status)) = status_result {
            Ok(status)
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to get status for transaction {status_result:?}"),
            ))
        }
    }

    /// returns a paginated set of transactions sorted by block height
    pub async fn transactions(
        &self,
        request: PaginationRequest<String>,
    ) -> io::Result<PaginatedResult<TransactionResponse, String>> {
        let args = schema::ConnectionArgs::from(request);
        let query = schema::tx::TransactionsQuery::build(args);
        let transactions = self.query(query).await?.transactions.try_into()?;
        Ok(transactions)
    }

    /// Returns a paginated set of transactions associated with a txo owner address.
    pub async fn transactions_by_owner(
        &self,
        owner: &Address,
        request: PaginationRequest<String>,
    ) -> io::Result<PaginatedResult<TransactionResponse, String>> {
        let owner: schema::Address = (*owner).into();
        let args = TransactionsByOwnerConnectionArgs::from((owner, request));
        let query = schema::tx::TransactionsByOwnerQuery::build(args);

        let transactions = self.query(query).await?.transactions_by_owner.try_into()?;
        Ok(transactions)
    }

    pub async fn receipts(&self, id: &TxId) -> io::Result<Option<Vec<Receipt>>> {
        let query =
            schema::tx::TransactionStatusQuery::build(TxIdArgs { id: (*id).into() });

        let tx = self.query(query).await?.transaction.ok_or_else(|| {
            io::Error::new(ErrorKind::NotFound, format!("transaction {id} not found"))
        })?;

        let receipts = match tx.status {
            Some(status) => match status {
                schema::tx::TransactionStatus::SuccessStatus(s) => Some(
                    s.receipts
                        .into_iter()
                        .map(TryInto::<Receipt>::try_into)
                        .collect::<Result<Vec<Receipt>, ConversionError>>(),
                )
                .transpose()?,
                schema::tx::TransactionStatus::FailureStatus(s) => Some(
                    s.receipts
                        .into_iter()
                        .map(TryInto::<Receipt>::try_into)
                        .collect::<Result<Vec<Receipt>, ConversionError>>(),
                )
                .transpose()?,
                _ => None,
            },
            _ => None,
        };

        Ok(receipts)
    }

    #[cfg(feature = "test-helpers")]
    pub async fn all_receipts(&self) -> io::Result<Vec<Receipt>> {
        let query = schema::tx::AllReceipts::build(());
        let receipts = self.query(query).await?.all_receipts;

        let vec: Result<Vec<Receipt>, ConversionError> = receipts
            .into_iter()
            .map(TryInto::<Receipt>::try_into)
            .collect();

        Ok(vec?)
    }

    pub async fn produce_blocks(
        &self,
        blocks_to_produce: u32,
        start_timestamp: Option<u64>,
    ) -> io::Result<BlockHeight> {
        let query = schema::block::BlockMutation::build(ProduceBlockArgs {
            blocks_to_produce: blocks_to_produce.into(),
            start_timestamp: start_timestamp
                .map(|timestamp| Tai64Timestamp::from(Tai64(timestamp))),
        });

        let new_height = self.query(query).await?.produce_blocks;

        Ok(new_height.into())
    }

    pub async fn block(&self, id: &BlockId) -> io::Result<Option<types::Block>> {
        let query = schema::block::BlockByIdQuery::build(BlockByIdArgs {
            id: Some((*id).into()),
        });

        let block = self
            .query(query)
            .await?
            .block
            .map(TryInto::try_into)
            .transpose()?;

        Ok(block)
    }

    pub async fn block_by_height(
        &self,
        height: BlockHeight,
    ) -> io::Result<Option<types::Block>> {
        let query = schema::block::BlockByHeightQuery::build(BlockByHeightArgs {
            height: Some(U32(height.into())),
        });

        let block = self
            .query(query)
            .await?
            .block
            .map(TryInto::try_into)
            .transpose()?;

        Ok(block)
    }

    pub async fn da_compressed_block(
        &self,
        height: BlockHeight,
    ) -> io::Result<Option<Vec<u8>>> {
        let query = schema::da_compressed::DaCompressedBlockByHeightQuery::build(
            DaCompressedBlockByHeightArgs {
                height: U32(height.into()),
            },
        );

        Ok(self
            .query(query)
            .await?
            .da_compressed_block
            .map(|b| b.bytes.into()))
    }

    /// Retrieve a blob by its ID
    pub async fn blob(&self, id: BlobId) -> io::Result<Option<types::Blob>> {
        let query = schema::blob::BlobByIdQuery::build(BlobByIdArgs { id: id.into() });
        let blob = self.query(query).await?.blob.map(Into::into);
        Ok(blob)
    }

    /// Check whether a blob with ID exists
    pub async fn blob_exists(&self, id: BlobId) -> io::Result<bool> {
        let query = schema::blob::BlobExistsQuery::build(BlobByIdArgs { id: id.into() });
        Ok(self.query(query).await?.blob.is_some())
    }

    /// Retrieve multiple blocks
    pub async fn blocks(
        &self,
        request: PaginationRequest<String>,
    ) -> io::Result<PaginatedResult<types::Block, String>> {
        let args = schema::ConnectionArgs::from(request);
        let query = schema::block::BlocksQuery::build(args);

        let blocks = self.query(query).await?.blocks.try_into()?;

        Ok(blocks)
    }

    pub async fn coin(&self, id: &UtxoId) -> io::Result<Option<types::Coin>> {
        let query = schema::coins::CoinByIdQuery::build(CoinByIdArgs {
            utxo_id: (*id).into(),
        });
        let coin = self.query(query).await?.coin.map(Into::into);
        Ok(coin)
    }

    /// Retrieve a page of coins by their owner
    pub async fn coins(
        &self,
        owner: &Address,
        asset_id: Option<&AssetId>,
        request: PaginationRequest<String>,
    ) -> io::Result<PaginatedResult<types::Coin, String>> {
        let owner: schema::Address = (*owner).into();
        let asset_id = asset_id.map(|id| (*id).into());
        let args = CoinsConnectionArgs::from((owner, asset_id, request));
        let query = schema::coins::CoinsQuery::build(args);

        let coins = self.query(query).await?.coins.into();
        Ok(coins)
    }

    /// Retrieve coins to spend in a transaction
    pub async fn coins_to_spend(
        &self,
        owner: &Address,
        spend_query: Vec<(AssetId, u128, Option<u16>)>,
        // (Utxos, Messages Nonce)
        excluded_ids: Option<(Vec<UtxoId>, Vec<Nonce>)>,
    ) -> io::Result<Vec<Vec<types::CoinType>>> {
        let owner: schema::Address = (*owner).into();
        let spend_query: Vec<SpendQueryElementInput> = spend_query
            .iter()
            .map(|(asset_id, amount, max)| -> Result<_, ConversionError> {
                Ok(SpendQueryElementInput {
                    asset_id: (*asset_id).into(),
                    amount: (*amount).into(),
                    max: (*max).map(|max| max.into()),
                })
            })
            .try_collect()?;
        let excluded_ids: Option<ExcludeInput> = excluded_ids.map(Into::into);
        let args =
            schema::coins::CoinsToSpendArgs::from((owner, spend_query, excluded_ids));
        let query = schema::coins::CoinsToSpendQuery::build(args);

        let coins_per_asset = self
            .query(query)
            .await?
            .coins_to_spend
            .into_iter()
            .map(|v| v.into_iter().map(Into::into).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        Ok(coins_per_asset)
    }

    pub async fn contract(&self, id: &ContractId) -> io::Result<Option<types::Contract>> {
        let query = schema::contract::ContractByIdQuery::build(ContractByIdArgs {
            id: (*id).into(),
        });
        let contract = self.query(query).await?.contract.map(Into::into);
        Ok(contract)
    }

    pub async fn contract_balance(
        &self,
        id: &ContractId,
        asset: Option<&AssetId>,
    ) -> io::Result<u64> {
        let asset_id: schema::AssetId = match asset {
            Some(asset) => (*asset).into(),
            None => schema::AssetId::default(),
        };

        let query =
            schema::contract::ContractBalanceQuery::build(ContractBalanceQueryArgs {
                id: (*id).into(),
                asset: asset_id,
            });

        let balance: types::ContractBalance =
            self.query(query).await?.contract_balance.into();
        Ok(balance.amount)
    }

    pub async fn balance(
        &self,
        owner: &Address,
        asset_id: Option<&AssetId>,
    ) -> io::Result<u128> {
        let owner: schema::Address = (*owner).into();
        let asset_id: schema::AssetId = match asset_id {
            Some(asset_id) => (*asset_id).into(),
            None => schema::AssetId::default(),
        };
        let query = schema::balance::BalanceQuery::build(BalanceArgs { owner, asset_id });
        let balance: types::Balance = self.query(query).await?.balance.into();
        Ok(balance.amount)
    }

    // Retrieve a page of balances by their owner
    pub async fn balances(
        &self,
        owner: &Address,
        request: PaginationRequest<String>,
    ) -> io::Result<PaginatedResult<types::Balance, String>> {
        let owner: schema::Address = (*owner).into();
        let args = schema::balance::BalancesConnectionArgs::from((owner, request));
        let query = schema::balance::BalancesQuery::build(args);

        let balances = self.query(query).await?.balances.into();
        Ok(balances)
    }

    pub async fn contract_balances(
        &self,
        contract: &ContractId,
        request: PaginationRequest<String>,
    ) -> io::Result<PaginatedResult<types::ContractBalance, String>> {
        let contract_id: schema::ContractId = (*contract).into();
        let args = ContractBalancesConnectionArgs::from((contract_id, request));
        let query = schema::contract::ContractBalancesQuery::build(args);

        let balances = self.query(query).await?.contract_balances.into();

        Ok(balances)
    }

    // Retrieve a message by its nonce
    pub async fn message(&self, nonce: &Nonce) -> io::Result<Option<types::Message>> {
        let query = schema::message::MessageQuery::build(NonceArgs {
            nonce: (*nonce).into(),
        });
        let message = self.query(query).await?.message.map(Into::into);
        Ok(message)
    }

    pub async fn messages(
        &self,
        owner: Option<&Address>,
        request: PaginationRequest<String>,
    ) -> io::Result<PaginatedResult<types::Message, String>> {
        let owner: Option<schema::Address> = owner.map(|owner| (*owner).into());
        let args = schema::message::OwnedMessagesConnectionArgs::from((owner, request));
        let query = schema::message::OwnedMessageQuery::build(args);

        let messages = self.query(query).await?.messages.into();

        Ok(messages)
    }

    pub async fn contract_info(
        &self,
        contract: &ContractId,
    ) -> io::Result<Option<types::Contract>> {
        let query = schema::contract::ContractByIdQuery::build(ContractByIdArgs {
            id: (*contract).into(),
        });
        let contract_info = self.query(query).await?.contract.map(Into::into);
        Ok(contract_info)
    }

    pub async fn message_status(&self, nonce: &Nonce) -> io::Result<MessageStatus> {
        let query = schema::message::MessageStatusQuery::build(MessageStatusArgs {
            nonce: (*nonce).into(),
        });
        let status = self.query(query).await?.message_status.into();

        Ok(status)
    }

    /// Request a merkle proof of an output message.
    pub async fn message_proof(
        &self,
        transaction_id: &TxId,
        nonce: &Nonce,
        commit_block_id: Option<&BlockId>,
        commit_block_height: Option<BlockHeight>,
    ) -> io::Result<types::MessageProof> {
        let transaction_id: TransactionId = (*transaction_id).into();
        let nonce: schema::Nonce = (*nonce).into();
        let commit_block_id: Option<schema::BlockId> =
            commit_block_id.map(|commit_block_id| (*commit_block_id).into());
        let commit_block_height = commit_block_height.map(Into::into);
        let query = schema::message::MessageProofQuery::build(MessageProofArgs {
            transaction_id,
            nonce,
            commit_block_id,
            commit_block_height,
        });
        let proof = self.query(query).await?.message_proof.try_into()?;
        Ok(proof)
    }

    pub async fn relayed_transaction_status(
        &self,
        id: &Bytes32,
    ) -> io::Result<Option<RelayedTransactionStatus>> {
        let query = schema::relayed_tx::RelayedTransactionStatusQuery::build(
            RelayedTransactionStatusArgs {
                id: id.to_owned().into(),
            },
        );
        let status = self
            .query(query)
            .await?
            .relayed_transaction_status
            .map(|status| status.try_into())
            .transpose()?;
        Ok(status)
    }

    pub async fn asset_info(&self, asset_id: &AssetId) -> io::Result<AssetDetail> {
        let query = schema::assets::AssetInfoQuery::build(AssetInfoArg {
            id: (*asset_id).into(),
        });
        let asset_info = self.query(query).await?.asset_details.into();
        Ok(asset_info)
    }
}

#[cfg(any(test, feature = "test-helpers"))]
impl FuelClient {
    pub async fn transparent_transaction(
        &self,
        id: &TxId,
    ) -> io::Result<Option<types::TransactionType>> {
        let query = schema::tx::TransactionQuery::build(TxIdArgs { id: (*id).into() });

        let transaction = self.query(query).await?.transaction;

        Ok(transaction
            .map(|tx| {
                let response: TransactionResponse = tx.try_into()?;
                Ok::<_, ConversionError>(response.transaction)
            })
            .transpose()?)
    }
}
