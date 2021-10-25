use cynic::http::SurfExt;
use cynic::{MutationBuilder, Operation, QueryBuilder};
use fuel_vm::prelude::*;
use std::convert::TryInto;
use std::str::{self, FromStr};
use std::{io, net};

pub mod schema;

use crate::client::schema::coin::{Coin, CoinByIdArgs, CoinConnection, CoinsByOwnerConnectionArgs};
use crate::client::schema::tx::TxArg;
use schema::{
    block::{BlockByIdArgs, BlockConnection},
    tx::TxIdArgs,
    *,
};

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
                let e = e.into_iter().map(|e| format!("{}", e.message)).fold(
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
        let tx = serde_json::to_string(tx)?;
        let query = schema::tx::DryRun::build(&TxArg { tx });

        let receipts = self.query(query).await.map(|r| r.dry_run)?;
        receipts
            .into_iter()
            .map(|receipt| receipt.try_into().map_err(Into::into))
            .collect()
    }

    pub async fn submit(&self, tx: &Transaction) -> io::Result<HexString256> {
        let tx = serde_json::to_string(tx)?;
        let query = schema::tx::Submit::build(&TxArg { tx });

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
            register: register as i32,
        });

        Ok(self.query(query).await?.register as Word)
    }

    pub async fn memory(&self, id: &str, start: usize, size: usize) -> io::Result<Vec<u8>> {
        let query = schema::Memory::build(&MemoryArgs {
            id: id.into(),
            start: start as i32,
            size: size as i32,
        });

        let memory = self.query(query).await?.memory;

        Ok(serde_json::from_str(memory.as_str())?)
    }

    pub async fn transaction(&self, id: &str) -> io::Result<Option<fuel_tx::Transaction>> {
        let query = schema::tx::TransactionQuery::build(&TxIdArgs { id: id.parse()? });

        let transaction = self.query(query).await?.transaction;

        Ok(transaction.map(|tx| tx.try_into()).transpose()?)
    }
    //
    // pub async fn receipts(&self, id: &HexString256) -> io::Result<Option<Vec<fuel_tx::Receipt>>> {
    //     let query = schema::tx::TransactionQuery::build(&TxIdArgs { id: id.clone() });
    //
    //     let tx = self.query(query).await?.transaction.ok_or(|| );
    //
    //     let receipts = tx.map(|tx| tx.receipts.into_iter().map(|r| r.try_into()).collect());
    //
    //     Ok(receipts.transpose()?)
    // }

    pub async fn block(&self, id: &str) -> io::Result<Option<schema::block::Block>> {
        let query = schema::block::BlockByIdQuery::build(&BlockByIdArgs { id: id.parse()? });

        let block = self.query(query).await?.block;

        Ok(block)
    }

    pub async fn blocks(
        &self,
        first: Option<i32>,
        last: Option<i32>,
        before: Option<String>,
        after: Option<String>,
    ) -> io::Result<BlockConnection> {
        let query = schema::block::BlocksQuery::build(&ConnectionArgs {
            after,
            before,
            first,
            last,
        });

        let blocks = self.query(query).await?.blocks;

        Ok(blocks)
    }

    pub async fn coin(&self, id: &str) -> io::Result<Option<Coin>> {
        let query = schema::coin::CoinByIdQuery::build(CoinByIdArgs { id: id.parse()? });
        let coin = self.query(query).await?.coin;
        Ok(coin)
    }

    pub async fn coins_by_owner(
        &self,
        owner: &str,
        first: Option<i32>,
        last: Option<i32>,
        before: Option<String>,
        after: Option<String>,
    ) -> io::Result<CoinConnection> {
        let query = schema::coin::CoinsQuery::build(&CoinsByOwnerConnectionArgs {
            owner: owner.parse()?,
            after,
            before,
            first,
            last,
        });

        let coins = self.query(query).await?.coins_by_owner;
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
