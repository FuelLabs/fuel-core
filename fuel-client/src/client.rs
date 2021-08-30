use cynic::http::SurfExt;
use cynic::{MutationBuilder, Operation, QueryBuilder};

use fuel_vm::prelude::*;
use std::str::{self, FromStr};
use std::{io, net};

mod schema;

use schema::*;

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

    pub async fn transact(&self, tx: &Transaction) -> io::Result<Vec<Receipt>> {
        let tx = serde_json::to_string(tx)?;
        let query = schema::Run::build(&TxArg { tx });

        let result = self.query(query).await.map(|r| r.run)?;
        let result = serde_json::from_str(result.as_str())?;

        Ok(result)
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
}
