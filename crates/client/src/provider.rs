use crate::client::schema::gas_price::{BlockHorizonArgs, EstimateGasPrice};
use crate::client::schema::tx::DryRunArg;
use crate::client::schema::{Bytes, HexString};
use crate::client::types::gas_price::LatestGasPrice;
use crate::client::types::primitives::Bytes32;
use crate::client::types::upgrades::StateTransitionBytecode;
use crate::client::{from_strings_errors_to_std_error, schema, types};
use crate::reqwest_ext::{FuelGraphQlResponse, FuelOperation, ReqwestExt};
use async_trait::async_trait;
use cynic::MutationBuilder;
use cynic::{Operation, QueryBuilder};
use fuel_core_types::fuel_tx::{ConsensusParameters, Transaction};
use fuel_core_types::fuel_types::canonical::Serialize;
use fuel_core_types::fuel_types::BlockHeight;
use fuel_core_types::services::executor::{StorageReadReplayEvent, TransactionExecutionStatus};
use serde::de::DeserializeOwned;
use std::fmt::Debug;
use std::io;
use std::sync::Arc;

#[async_trait]
pub trait Provider: Debug + Clone + Send + Sync + 'static {
    async fn query<ResponseData, Vars>(
        &self,
        q: Operation<ResponseData, Vars>,
    ) -> io::Result<ResponseData>
    where
        Vars: serde::Serialize + Send + 'static,
        ResponseData: serde::de::DeserializeOwned + Send + 'static;

    fn required_block_height(&self) -> Option<BlockHeight>;

    async fn health(&self) -> io::Result<bool> {
        let query = schema::Health::build(());
        self.query(query).await.map(|r| r.health)
    }

    async fn node_info(&self) -> io::Result<types::NodeInfo> {
        let query = schema::node_info::QueryNodeInfo::build(());
        self.query(query).await.map(|r| r.node_info.into())
    }

    async fn latest_gas_price(&self) -> io::Result<LatestGasPrice> {
        let query = schema::gas_price::QueryLatestGasPrice::build(());
        self.query(query).await.map(|r| r.latest_gas_price.into())
    }

    async fn estimate_gas_price(
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
    async fn connected_peers_info(
        &self,
    ) -> io::Result<Vec<fuel_core_types::services::p2p::PeerInfo>> {
        let query = schema::node_info::QueryPeersInfo::build(());
        self.query(query)
            .await
            .map(|r| r.node_info.peers.into_iter().map(Into::into).collect())
    }

    async fn chain_info(&self) -> io::Result<types::ChainInfo> {
        let query = schema::chain::ChainQuery::build(());
        self.query(query).await.and_then(|r| {
            let result = r.chain.try_into()?;
            Ok(result)
        })
    }

    async fn consensus_parameters(
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

    async fn state_transition_byte_code_by_root(
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
    async fn dry_run(
        &self,
        txs: &[Transaction],
    ) -> io::Result<Vec<TransactionExecutionStatus>> {
        self.dry_run_opt(txs, None, None, None).await
    }

    /// Dry run with options to override the node behavior
    async fn dry_run_opt(
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
    async fn dry_run_opt_record_storage_reads(
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
    fn decode_response<R, E>(&self, response: FuelGraphQlResponse<R, E>) -> io::Result<R>
    where
        R: serde::de::DeserializeOwned + 'static,
    {
        if response
            .extensions
            .as_ref()
            .and_then(|e| e.fuel_block_height_precondition_failed)
            == Some(true)
        {
            return Err(io::Error::other("The required block height was not met"));
        }

        let response = response.response;

        match (response.data, response.errors) {
            (Some(d), _) => Ok(d),
            (_, Some(e)) => Err(from_strings_errors_to_std_error(
                e.into_iter().map(|e| e.message).collect(),
            )),
            _ => Err(io::Error::other("Invalid response")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReqwestProvider {
    client: reqwest::Client,
    url: reqwest::Url,
}

impl ReqwestProvider {
    pub fn new(url: reqwest::Url, cookie: Arc<reqwest::cookie::Jar>) -> reqwest::Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .cookie_provider(cookie.clone())
                .build()?,
            url,
        })
    }

    pub fn set_url(&mut self, url: reqwest::Url) {
        self.url = url;
    }
}

#[async_trait]
impl Provider for ReqwestProvider {
    async fn query<ResponseData, Vars>(&self, q: Operation<ResponseData, Vars>) -> io::Result<ResponseData>
    where
        Vars: serde::Serialize + Send + 'static,
        ResponseData: DeserializeOwned + Send + 'static,
    {
        let required_fuel_block_height = self.required_block_height();
        let fuel_operation = FuelOperation::new(q, required_fuel_block_height);
        let response = self
            .client
            .post(self.url.clone())
            .run_fuel_graphql(fuel_operation)
            .await
            .map_err(io::Error::other)?;

        self.decode_response(response)
    }

    fn required_block_height(&self) -> Option<BlockHeight> {
        todo!()
    }
}

#[derive(Debug, Clone)]
struct FailoverProvider {
    client: ReqwestProvider,
    urls: Vec<reqwest::Url>,
}

impl FailoverProvider {
    pub fn new(urls: Vec<reqwest::Url>, cookie: Arc<reqwest::cookie::Jar>) -> reqwest::Result<Self> {
        Ok(Self {
            client: ReqwestProvider::new(urls.first().unwrap().clone(), cookie)?,
            urls,
        })
    }
}

#[async_trait]
impl Provider for FailoverProvider {
    async fn query<ResponseData, Vars>(&self, q: Operation<ResponseData, Vars>) -> io::Result<ResponseData>
    where
        Vars: serde::Serialize + Send + 'static,
        ResponseData: DeserializeOwned + Send + 'static,
    {
        for url in &self.urls {
            self.client.set_url(url.clone());
            if let Ok(resp) = self.client.query(q.clone()).await {
                return Ok(resp);
            }
        }
        Err(todo!("Failover provider not implemented"))
    }

    fn required_block_height(&self) -> Option<BlockHeight> {
        todo!()
    }
}