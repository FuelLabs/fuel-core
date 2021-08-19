use cynic::http::SurfExt;
use cynic::{MutationBuilder, Operation};

use std::str::{self, FromStr};
use std::{io, net};

use fuel_vm::prelude::*;

mod schema;

use schema::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TxClient {
    url: surf::Url,
}

impl FromStr for TxClient {
    type Err = net::AddrParseError;

    fn from_str(str: &str) -> Result<Self, Self::Err> {
        str.parse().map(|s: net::SocketAddr| s.into())
    }
}

impl<S> From<S> for TxClient
where
    S: Into<net::SocketAddr>,
{
    fn from(socket: S) -> Self {
        let url = format!("http://{}/tx", socket.into())
            .as_str()
            .parse()
            .unwrap();

        Self { url }
    }
}

impl TxClient {
    pub fn new(url: impl AsRef<str>) -> Result<Self, net::AddrParseError> {
        Self::from_str(url.as_ref())
    }

    async fn query<'a, R: 'a>(&self, q: Operation<'a, R>) -> io::Result<R> {
        let response = surf::post(&self.url)
            .run_graphql(q)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        if let Some(errors) = response.errors {
            Err(io::Error::new(
                io::ErrorKind::Other,
                format!("{:?}", errors),
            ))
        } else if let Some(data) = response.data {
            Ok(data)
        } else {
            Err(io::Error::new(io::ErrorKind::NotFound, "Invalid response"))
        }
    }

    pub async fn transact(&self, tx: &Transaction) -> io::Result<Vec<LogEvent>> {
        let tx = serde_json::to_string(tx)?;
        let query = schema::Run::build(&TxArg { tx });

        let result = self.query(query).await.map(|r| r.run)?;
        let result = serde_json::from_str(result.as_str())?;

        Ok(result)
    }
}
