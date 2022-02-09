use cynic::{http::SurfExt, MutationBuilder, Operation, QueryBuilder};
use fuel_vm::prelude::*;
use itertools::Itertools;
use std::{
    convert::TryInto,
    io, net,
    str::{self, FromStr},
};

pub mod schema;
pub mod types;

use schema::{
    block::BlockByIdArgs,
    coin::{Coin, CoinByIdArgs},
    tx::{TxArg, TxIdArgs},
    Bytes, HexString, HexString256, IdArg, MemoryArgs, RegisterArgs,
};

use crate::client::schema::ConversionError;
use crate::client::types::{TransactionResponse, TransactionStatus};
pub use schema::{PageDirection, PaginatedResult, PaginationRequest};
use std::io::ErrorKind;

use self::schema::coin::SpendQueryElementInput;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FuelClient {
    url: surf::Url,
}

impl FromStr for FuelClient {
    type Err = net::AddrParseError;

    fn from_str(str: &str) -> Result<Self, Self::Err> {
        str.parse().map(|s: net::SocketAddr| s.into())
    }
}

impl<S> From<S> for FuelClient
where
    S: Into<net::SocketAddr>,
{
    fn from(socket: S) -> Self {
        let url = format!("http://{}/graphql", socket.into())
            .as_str()
            .parse()
            .unwrap();

        Self { url }
    }
}

impl FuelClient {
    pub fn new(url: impl AsRef<str>) -> Result<Self, net::AddrParseError> {
        Self::from_str(url.as_ref())
    }

    async fn query<'a, R: 'a>(&self, q: Operation<'a, R>) -> io::Result<R> {
        let response = surf::post(&self.url)
            .run_graphql(q)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        match (response.data, response.errors) {
            (Some(d), _) => Ok(d),
            (_, Some(e)) => {
                let e = e.into_iter().map(|e| e.message).fold(
                    String::from("Response errors"),
                    |mut s, e| {
                        s.push_str("; ");
                        s.push_str(e.as_str());
                        s
                    },
                );
                Err(io::Error::new(io::ErrorKind::Other, e))
            }
            _ => Err(io::Error::new(io::ErrorKind::Other, "Invalid response")),
        }
    }

    pub async fn health(&self) -> io::Result<bool> {
        let query = schema::Health::build(());
        self.query(query).await.map(|r| r.health)
    }

    pub async fn dry_run(&self, tx: &Transaction) -> io::Result<Vec<Receipt>> {
        let tx = tx.clone().to_bytes();
        let query = schema::tx::DryRun::build(&TxArg {
            tx: HexString(Bytes(tx)),
        });
        let receipts = self.query(query).await.map(|r| r.dry_run)?;
        receipts
            .into_iter()
            .map(|receipt| receipt.try_into().map_err(Into::into))
            .collect()
    }

    pub async fn submit(&self, tx: &Transaction) -> io::Result<HexString256> {
        let tx = tx.clone().to_bytes();
        let query = schema::tx::Submit::build(&TxArg {
            tx: HexString(Bytes(tx)),
        });

        let id = self.query(query).await.map(|r| r.submit)?;
        Ok(id)
    }

    pub async fn start_session(&self) -> io::Result<String> {
        let query = schema::StartSession::build(&());

        self.query(query)
            .await
            .map(|r| r.start_session.into_inner())
    }

    pub async fn end_session(&self, id: &str) -> io::Result<bool> {
        let query = schema::EndSession::build(&IdArg { id: id.into() });

        self.query(query).await.map(|r| r.end_session)
    }

    pub async fn reset(&self, id: &str) -> io::Result<bool> {
        let query = schema::Reset::build(&IdArg { id: id.into() });

        self.query(query).await.map(|r| r.reset)
    }

    pub async fn execute(&self, id: &str, op: &Opcode) -> io::Result<bool> {
        let op = serde_json::to_string(op)?;
        let query = schema::Execute::build(&schema::ExecuteArgs { id: id.into(), op });

        self.query(query).await.map(|r| r.execute)
    }

    pub async fn register(&self, id: &str, register: RegisterId) -> io::Result<Word> {
        let query = schema::Register::build(&RegisterArgs {
            id: id.into(),
            register: register.into(),
        });

        Ok(self.query(query).await?.register.0 as Word)
    }

    pub async fn memory(&self, id: &str, start: usize, size: usize) -> io::Result<Vec<u8>> {
        let query = schema::Memory::build(&MemoryArgs {
            id: id.into(),
            start: start.into(),
            size: size.into(),
        });

        let memory = self.query(query).await?.memory;

        Ok(serde_json::from_str(memory.as_str())?)
    }

    pub async fn transaction(&self, id: &str) -> io::Result<Option<TransactionResponse>> {
        let query = schema::tx::TransactionQuery::build(&TxIdArgs { id: id.parse()? });

        let transaction = self.query(query).await?.transaction;

        Ok(transaction.map(|tx| tx.try_into()).transpose()?)
    }

    /// Get the status of a transaction
    pub async fn transaction_status(&self, id: &str) -> io::Result<TransactionStatus> {
        let query = schema::tx::TransactionQuery::build(&TxIdArgs { id: id.parse()? });

        let tx = self.query(query).await?.transaction.ok_or_else(|| {
            io::Error::new(ErrorKind::NotFound, format!("transaction {} not found", id))
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

    /// returns a paginated set of transactions sorted by block height
    pub async fn transactions(
        &self,
        request: PaginationRequest<String>,
    ) -> io::Result<PaginatedResult<TransactionResponse, String>> {
        let query = schema::tx::TransactionsQuery::build(&request.into());
        let transactions = self.query(query).await?.transactions.try_into()?;
        Ok(transactions)
    }

    /// Returns a paginated set of transactions associated with a txo owner address.
    pub async fn transactions_by_owner(
        &self,
        owner: &str,
        request: PaginationRequest<String>,
    ) -> io::Result<PaginatedResult<TransactionResponse, String>> {
        let owner: HexString256 = owner.parse()?;
        let query = schema::tx::TransactionsByOwnerQuery::build(&(owner, request).into());

        let transactions = self.query(query).await?.transactions_by_owner.try_into()?;
        Ok(transactions)
    }

    pub async fn receipts(&self, id: &str) -> io::Result<Vec<fuel_tx::Receipt>> {
        let query = schema::tx::TransactionQuery::build(&TxIdArgs { id: id.parse()? });

        let tx = self.query(query).await?.transaction.ok_or_else(|| {
            io::Error::new(ErrorKind::NotFound, format!("transaction {} not found", id))
        })?;

        let receipts: Result<Vec<fuel_tx::Receipt>, ConversionError> = tx
            .receipts
            .unwrap_or_default()
            .into_iter()
            .map(|r| r.try_into())
            .collect();

        Ok(receipts?)
    }

    pub async fn block(&self, id: &str) -> io::Result<Option<schema::block::Block>> {
        let query = schema::block::BlockByIdQuery::build(&BlockByIdArgs { id: id.parse()? });

        let block = self.query(query).await?.block;

        Ok(block)
    }

    /// Retrieve multiple blocks
    pub async fn blocks(
        &self,
        request: PaginationRequest<String>,
    ) -> io::Result<PaginatedResult<schema::block::Block, String>> {
        let query = schema::block::BlocksQuery::build(&request.into());

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
        color: Option<&str>,
        request: PaginationRequest<String>,
    ) -> io::Result<PaginatedResult<schema::coin::Coin, String>> {
        let owner: HexString256 = owner.parse()?;
        let color: HexString256 = match color {
            Some(color) => color.parse()?,
            None => HexString256::default(),
        };
        let query = schema::coin::CoinsQuery::build(&(owner, color, request).into());

        let coins = self.query(query).await?.coins.into();
        Ok(coins)
    }

    /// Retrieve coins to spend in a transaction
    pub async fn coins_to_spend(
        &self,
        owner: &str,
        spend_query: Vec<(&str, u64)>,
        max_inputs: Option<i32>,
    ) -> io::Result<Vec<schema::coin::Coin>> {
        let owner: HexString256 = owner.parse()?;
        let spend_query: Vec<SpendQueryElementInput> = spend_query
            .iter()
            .map(|(color, amount)| -> Result<_, ConversionError> {
                Ok(SpendQueryElementInput {
                    color: color.parse()?,
                    amount: (*amount).into(),
                })
            })
            .try_collect()?;
        let query =
            schema::coin::CoinsToSpendQuery::build(&(owner, spend_query, max_inputs).into());

        let coins = self.query(query).await?.coins_to_spend;
        Ok(coins)
    }
}

#[cfg(any(test, feature = "test-helpers"))]
impl FuelClient {
    pub async fn transparent_transaction(
        &self,
        id: &str,
    ) -> io::Result<Option<fuel_tx::Transaction>> {
        let query = schema::tx::TransactionQuery::build(&TxIdArgs { id: id.parse()? });

        let transaction = self.query(query).await?.transaction;

        Ok(transaction.map(|tx| tx.try_into()).transpose()?)
    }
}
