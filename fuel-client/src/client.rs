use crate::client::schema::contract::ContractBalanceQueryArgs;
use anyhow::Context;
use cynic::{
    http::ReqwestExt,
    GraphQlResponse,
    Id,
    MutationBuilder,
    Operation,
    QueryBuilder,
    StreamingOperation,
};
use fuel_vm::prelude::*;
use futures::StreamExt;
use itertools::Itertools;
use schema::{
    balance::BalanceArgs,
    block::BlockByIdArgs,
    coin::{
        Coin,
        CoinByIdArgs,
    },
    contract::{
        Contract,
        ContractByIdArgs,
    },
    resource::SpendQueryElementInput,
    tx::{
        TxArg,
        TxIdArgs,
    },
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
    TransactionId,
    U64,
};
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
};
use types::{
    TransactionResponse,
    TransactionStatus,
};

use crate::client::schema::{
    resource::ExcludeInput,
    tx::DryRunArg,
};
pub use schema::{
    PageDirection,
    PaginatedResult,
    PaginationRequest,
};

use self::schema::{
    block::{
        ProduceBlockArgs,
        TimeParameters,
    },
    message::MessageProofArgs,
};

pub mod schema;
pub mod types;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FuelClient {
    url: reqwest::Url,
}

impl FromStr for FuelClient {
    type Err = anyhow::Error;

    fn from_str(str: &str) -> Result<Self, Self::Err> {
        let mut raw_url = str.to_string();
        if !raw_url.starts_with("http") {
            raw_url = format!("http://{}", raw_url);
        }

        let mut url = reqwest::Url::parse(&raw_url)
            .with_context(|| format!("Invalid fuel-core URL: {}", str))?;
        url.set_path("/graphql");
        Ok(Self { url })
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

    async fn query<ResponseData, Vars>(
        &self,
        q: Operation<ResponseData, Vars>,
    ) -> io::Result<ResponseData>
    where
        Vars: serde::Serialize,
        ResponseData: serde::de::DeserializeOwned + 'static,
    {
        let response = reqwest::Client::new()
            .post(self.url.clone())
            .run_graphql(q)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        Self::decode_response(response)
    }

    fn decode_response<R>(response: GraphQlResponse<R>) -> io::Result<R>
    where
        R: serde::de::DeserializeOwned + 'static,
    {
        match (response.data, response.errors) {
            (Some(d), _) => Ok(d),
            (_, Some(e)) => Err(from_strings_errors_to_std_error(
                e.into_iter().map(|e| e.message).collect(),
            )),
            _ => Err(io::Error::new(io::ErrorKind::Other, "Invalid response")),
        }
    }

    async fn subscribe<ResponseData, Vars>(
        &self,
        q: StreamingOperation<ResponseData, Vars>,
    ) -> io::Result<impl futures::Stream<Item = io::Result<ResponseData>>>
    where
        Vars: serde::Serialize,
        ResponseData: serde::de::DeserializeOwned + 'static,
    {
        use eventsource_client as es;
        let mut url = self.url.clone();
        url.set_path("/graphql-sub");
        let json_query = serde_json::to_string(&q)?;
        let client = es::ClientBuilder::for_url(url.as_str())
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to start client {:?}", e),
                )
            })?
            .body(json_query)
            .method("POST".to_string())
            .header("content-type", "application/json")
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to add header to client {:?}", e),
                )
            })?
            .build_http();

        let mut last = None;

        let stream = es::Client::stream(&client)
            .take_while(|result| {
                futures::future::ready(!matches!(result, Err(es::Error::Eof)))
            })
            .filter_map(move |result| {
                let r = match result {
                    Ok(es::SSE::Event(es::Event { data, .. })) => {
                        match serde_json::from_str::<GraphQlResponse<ResponseData>>(&data)
                        {
                            Ok(resp) => {
                                match Self::decode_response(resp) {
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
                                        format!("Decode error: {:?}", e),
                                    ))),
                                }
                            }
                            Err(e) => Some(Err(io::Error::new(
                                io::ErrorKind::Other,
                                format!("Json error: {:?}", e),
                            ))),
                        }
                    }
                    Ok(_) => None,
                    Err(e) => Some(Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("Graphql error: {:?}", e),
                    ))),
                };
                futures::future::ready(r)
            });

        Ok(stream)
    }

    pub async fn health(&self) -> io::Result<bool> {
        let query = schema::Health::build(());
        self.query(query).await.map(|r| r.health)
    }

    pub async fn node_info(&self) -> io::Result<schema::node_info::NodeInfo> {
        let query = schema::node_info::QueryNodeInfo::build(());
        self.query(query).await.map(|r| r.node_info)
    }

    pub async fn chain_info(&self) -> io::Result<schema::chain::ChainInfo> {
        let query = schema::chain::ChainQuery::build(());
        self.query(query).await.map(|r| r.chain)
    }

    /// Default dry run, matching the exact configuration as the node
    pub async fn dry_run(&self, tx: &Transaction) -> io::Result<Vec<Receipt>> {
        self.dry_run_opt(tx, None).await
    }

    /// Dry run with options to override the node behavior
    pub async fn dry_run_opt(
        &self,
        tx: &Transaction,
        // Disable utxo input checks (exists, unspent, and valid signature)
        utxo_validation: Option<bool>,
    ) -> io::Result<Vec<Receipt>> {
        let tx = tx.clone().to_bytes();
        let query = schema::tx::DryRun::build(DryRunArg {
            tx: HexString(Bytes(tx)),
            utxo_validation,
        });
        let receipts = self.query(query).await.map(|r| r.dry_run)?;
        receipts
            .into_iter()
            .map(|receipt| receipt.try_into().map_err(Into::into))
            .collect()
    }

    pub async fn submit(&self, tx: &Transaction) -> io::Result<TransactionId> {
        let tx = tx.clone().to_bytes();
        let query = schema::tx::Submit::build(TxArg {
            tx: HexString(Bytes(tx)),
        });

        let id = self.query(query).await.map(|r| r.submit)?.id;
        Ok(id)
    }

    /// Submit the transaction and wait for it to be included into a block.
    ///
    /// This will wait forever if needed, so consider wrapping this call
    /// with a `tokio::time::timeout`.
    pub async fn submit_and_await_commit(
        &self,
        tx: &Transaction,
    ) -> io::Result<TransactionStatus> {
        let tx_id = self.submit(tx).await?;
        self.await_transaction_commit(&tx_id.to_string()).await
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

    pub async fn execute(&self, id: &str, op: &Opcode) -> io::Result<bool> {
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

    pub async fn memory(
        &self,
        id: &str,
        start: usize,
        size: usize,
    ) -> io::Result<Vec<u8>> {
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
        contract: ::fuel_vm::fuel_types::ContractId,
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

    pub async fn transaction(&self, id: &str) -> io::Result<Option<TransactionResponse>> {
        let query = schema::tx::TransactionQuery::build(TxIdArgs { id: id.parse()? });

        let transaction = self.query(query).await?.transaction;

        Ok(transaction.map(|tx| tx.try_into()).transpose()?)
    }

    /// Get the status of a transaction
    pub async fn transaction_status(&self, id: &str) -> io::Result<TransactionStatus> {
        let query = schema::tx::TransactionQuery::build(TxIdArgs { id: id.parse()? });

        let tx = self.query(query).await?.transaction.ok_or_else(|| {
            io::Error::new(
                ErrorKind::NotFound,
                format!("status not found for transaction {} ", id),
            )
        })?;

        let status = tx
            .status
            .ok_or_else(|| {
                io::Error::new(
                    ErrorKind::NotFound,
                    format!("status not found for transaction {}", id),
                )
            })?
            .try_into()?;
        Ok(status)
    }

    /// Subscribe to the status of a transaction
    pub async fn subscribe_transaction_status(
        &self,
        id: &str,
    ) -> io::Result<impl futures::Stream<Item = io::Result<TransactionStatus>>> {
        use cynic::SubscriptionBuilder;
        let s = schema::tx::StatusChangeSubscription::build(TxIdArgs { id: id.parse()? });

        let stream = self.subscribe(s).await?.map(|tx| {
            let tx = tx?;
            let status = tx.status_change.try_into()?;
            Ok(status)
        });

        Ok(stream)
    }

    /// Awaits for the transaction to be committed into a block
    ///
    /// This will wait forever if needed, so consider wrapping this call
    /// with a `tokio::time::timeout`.
    pub async fn await_transaction_commit(
        &self,
        id: &str,
    ) -> io::Result<TransactionStatus> {
        let mut outcomes: Vec<_> = futures::TryStreamExt::try_collect(
            self.subscribe_transaction_status(id).await?,
        )
        .await?;
        outcomes.pop().ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "Failed to get status for transaction")
        })
    }

    /// returns a paginated set of transactions sorted by block height
    pub async fn transactions(
        &self,
        request: PaginationRequest<String>,
    ) -> io::Result<PaginatedResult<TransactionResponse, String>> {
        let query = schema::tx::TransactionsQuery::build(request.into());
        let transactions = self.query(query).await?.transactions.try_into()?;
        Ok(transactions)
    }

    /// Returns a paginated set of transactions associated with a txo owner address.
    pub async fn transactions_by_owner(
        &self,
        owner: &str,
        request: PaginationRequest<String>,
    ) -> io::Result<PaginatedResult<TransactionResponse, String>> {
        let owner: schema::Address = owner.parse()?;
        let query = schema::tx::TransactionsByOwnerQuery::build((owner, request).into());

        let transactions = self.query(query).await?.transactions_by_owner.try_into()?;
        Ok(transactions)
    }

    pub async fn receipts(
        &self,
        id: &str,
    ) -> io::Result<Vec<::fuel_vm::fuel_tx::Receipt>> {
        let query = schema::tx::TransactionQuery::build(TxIdArgs { id: id.parse()? });

        let tx = self.query(query).await?.transaction.ok_or_else(|| {
            io::Error::new(ErrorKind::NotFound, format!("transaction {} not found", id))
        })?;

        let receipts: Result<Vec<::fuel_vm::fuel_tx::Receipt>, ConversionError> = tx
            .receipts
            .unwrap_or_default()
            .into_iter()
            .map(|r| r.try_into())
            .collect();

        Ok(receipts?)
    }

    pub async fn produce_blocks(
        &self,
        blocks_to_produce: u64,
        time: Option<TimeParameters>,
    ) -> io::Result<u64> {
        let query = schema::block::BlockMutation::build(ProduceBlockArgs {
            blocks_to_produce: blocks_to_produce.into(),
            time,
        });

        let new_height = self.query(query).await?.produce_blocks;

        Ok(new_height.into())
    }

    pub async fn block(&self, id: &str) -> io::Result<Option<schema::block::Block>> {
        let query =
            schema::block::BlockByIdQuery::build(BlockByIdArgs { id: id.parse()? });

        let block = self.query(query).await?.block;

        Ok(block)
    }

    /// Retrieve multiple blocks
    pub async fn blocks(
        &self,
        request: PaginationRequest<String>,
    ) -> io::Result<PaginatedResult<schema::block::Block, String>> {
        let query = schema::block::BlocksQuery::build(request.into());

        let blocks = self.query(query).await?.blocks.into();

        Ok(blocks)
    }

    pub async fn coin(&self, id: &str) -> io::Result<Option<Coin>> {
        let query = schema::coin::CoinByIdQuery::build(CoinByIdArgs {
            utxo_id: id.parse()?,
        });
        let coin = self.query(query).await?.coin;
        Ok(coin)
    }

    /// Retrieve a page of coins by their owner
    pub async fn coins(
        &self,
        owner: &str,
        asset_id: Option<&str>,
        request: PaginationRequest<String>,
    ) -> io::Result<PaginatedResult<schema::coin::Coin, String>> {
        let owner: schema::Address = owner.parse()?;
        let asset_id: schema::AssetId = match asset_id {
            Some(asset_id) => asset_id.parse()?,
            None => schema::AssetId::default(),
        };
        let query = schema::coin::CoinsQuery::build((owner, asset_id, request).into());

        let coins = self.query(query).await?.coins.into();
        Ok(coins)
    }

    /// Retrieve resources to spend in a transaction
    pub async fn resources_to_spend(
        &self,
        owner: &str,
        spend_query: Vec<(&str, u64, Option<u64>)>,
        // (Utxos, messages)
        excluded_ids: Option<(Vec<&str>, Vec<&str>)>,
    ) -> io::Result<Vec<Vec<schema::resource::Resource>>> {
        let owner: schema::Address = owner.parse()?;
        let spend_query: Vec<SpendQueryElementInput> = spend_query
            .iter()
            .map(|(asset_id, amount, max)| -> Result<_, ConversionError> {
                Ok(SpendQueryElementInput {
                    asset_id: asset_id.parse()?,
                    amount: (*amount).into(),
                    max: (*max).map(|max| max.into()),
                })
            })
            .try_collect()?;
        let excluded_ids: Option<ExcludeInput> =
            excluded_ids.map(ExcludeInput::from_tuple).transpose()?;
        let query = schema::resource::ResourcesToSpendQuery::build(
            (owner, spend_query, excluded_ids).into(),
        );

        let resources_per_asset = self.query(query).await?.resources_to_spend;
        Ok(resources_per_asset)
    }

    pub async fn contract(&self, id: &str) -> io::Result<Option<Contract>> {
        let query = schema::contract::ContractByIdQuery::build(ContractByIdArgs {
            id: id.parse()?,
        });
        let contract = self.query(query).await?.contract;
        Ok(contract)
    }

    pub async fn contract_balance(
        &self,
        id: &str,
        asset: Option<&str>,
    ) -> io::Result<u64> {
        let asset_id: schema::AssetId = match asset {
            Some(asset) => asset.parse()?,
            None => schema::AssetId::default(),
        };

        let query =
            schema::contract::ContractBalanceQuery::build(ContractBalanceQueryArgs {
                id: id.parse()?,
                asset: asset_id,
            });

        let balance = self.query(query).await.unwrap().contract_balance.amount;
        Ok(balance.into())
    }

    pub async fn balance(&self, owner: &str, asset_id: Option<&str>) -> io::Result<u64> {
        let owner: schema::Address = owner.parse()?;
        let asset_id: schema::AssetId = match asset_id {
            Some(asset_id) => asset_id.parse()?,
            None => schema::AssetId::default(),
        };
        let query = schema::balance::BalanceQuery::build(BalanceArgs { owner, asset_id });
        let balance = self.query(query).await?.balance;
        Ok(balance.amount.into())
    }

    // Retrieve a page of balances by their owner
    pub async fn balances(
        &self,
        owner: &str,
        request: PaginationRequest<String>,
    ) -> io::Result<PaginatedResult<schema::balance::Balance, String>> {
        let owner: schema::Address = owner.parse()?;
        let query = schema::balance::BalancesQuery::build((owner, request).into());

        let balances = self.query(query).await?.balances.into();
        Ok(balances)
    }

    pub async fn contract_balances(
        &self,
        contract: &str,
        request: PaginationRequest<String>,
    ) -> io::Result<PaginatedResult<schema::contract::ContractBalance, String>> {
        let contract_id: schema::ContractId = contract.parse()?;
        let query =
            schema::contract::ContractBalancesQuery::build((contract_id, request).into());

        let balances = self.query(query).await?.contract_balances.into();

        Ok(balances)
    }

    pub async fn messages(
        &self,
        owner: Option<&str>,
        request: PaginationRequest<String>,
    ) -> io::Result<PaginatedResult<schema::message::Message, String>> {
        let owner: Option<schema::Address> =
            owner.map(|owner| owner.parse()).transpose()?;
        let query = schema::message::OwnedMessageQuery::build((owner, request).into());

        let messages = self.query(query).await?.messages.into();

        Ok(messages)
    }

    /// Request a merkle proof of an output message.
    pub async fn message_proof(
        &self,
        transaction_id: &str,
        message_id: &str,
    ) -> io::Result<Option<schema::message::MessageProof>> {
        let transaction_id: schema::TransactionId = transaction_id.parse()?;
        let message_id: schema::MessageId = message_id.parse()?;
        let query = schema::message::MessageProofQuery::build(MessageProofArgs {
            transaction_id,
            message_id,
        });

        let proof = self.query(query).await?.message_proof;

        Ok(proof)
    }
}

#[cfg(any(test, feature = "test-helpers"))]
impl FuelClient {
    pub async fn transparent_transaction(
        &self,
        id: &str,
    ) -> io::Result<Option<::fuel_vm::fuel_tx::Transaction>> {
        let query = schema::tx::TransactionQuery::build(TxIdArgs { id: id.parse()? });

        let transaction = self.query(query).await?.transaction;

        Ok(transaction.map(|tx| tx.try_into()).transpose()?)
    }
}
