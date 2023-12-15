use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use futures03::prelude::Stream;
use reqwest::Client;
use serde_json::Value;
use slog::{debug, Logger};
use thiserror::Error;

use crate::cheap_clone::CheapClone;
use crate::data::subgraph::Link;
use crate::data_source::offchain::Base64;
use crate::prelude::Error;
use std::fmt::Debug;

/// The values that `json_stream` returns. The struct contains the deserialized
/// JSON value from the input stream, together with the line number from which
/// the value was read.
pub struct JsonStreamValue {
    pub value: Value,
    pub line: usize,
}

pub type JsonValueStream =
    Pin<Box<dyn Stream<Item = Result<JsonStreamValue, Error>> + Send + 'static>>;

/// Resolves links to subgraph manifests and resources referenced by them.
#[async_trait]
pub trait LinkResolver: Send + Sync + 'static + Debug {
    /// Updates the timeout used by the resolver.
    fn with_timeout(&self, timeout: Duration) -> Box<dyn LinkResolver>;

    /// Enables infinite retries.
    fn with_retries(&self) -> Box<dyn LinkResolver>;

    /// Fetches the link contents as bytes.
    async fn cat(&self, logger: &Logger, link: &Link) -> Result<Vec<u8>, Error>;

    /// Fetches the IPLD block contents as bytes.
    async fn get_block(&self, logger: &Logger, link: &Link) -> Result<Vec<u8>, Error>;

    /// Read the contents of `link` and deserialize them into a stream of JSON
    /// values. The values must each be on a single line; newlines are significant
    /// as they are used to split the file contents and each line is deserialized
    /// separately.
    async fn json_stream(&self, logger: &Logger, link: &Link) -> Result<JsonValueStream, Error>;
}

#[derive(Debug)]
pub struct ArweaveClient {
    base_url: url::Url,
    client: Client,
    logger: Logger,
}

#[derive(Debug, Clone)]
pub enum FileSizeLimit {
    Unlimited,
    MaxBytes(u64),
}

impl CheapClone for FileSizeLimit {}

impl Default for ArweaveClient {
    fn default() -> Self {
        use slog::o;

        Self {
            base_url: "https://arweave.net".parse().unwrap(),
            client: Client::default(),
            logger: Logger::root(slog::Discard, o!()),
        }
    }
}

impl ArweaveClient {
    pub fn new(logger: Logger, base_url: url::Url) -> Self {
        Self {
            base_url,
            logger,
            client: Client::default(),
        }
    }
}

#[async_trait]
impl ArweaveResolver for ArweaveClient {
    async fn get(&self, file: &Base64) -> Result<Vec<u8>, ArweaveClientError> {
        self.get_with_limit(file, &FileSizeLimit::Unlimited).await
    }

    async fn get_with_limit(
        &self,
        file: &Base64,
        limit: &FileSizeLimit,
    ) -> Result<Vec<u8>, ArweaveClientError> {
        let url = self.base_url.join(file.as_str())?;
        let rsp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(ArweaveClientError::from)?;

        match (&limit, rsp.content_length()) {
            (_, None) => return Err(ArweaveClientError::UnableToCheckFileSize),
            (FileSizeLimit::MaxBytes(max), Some(cl)) if cl > *max => {
                return Err(ArweaveClientError::FileTooLarge { got: cl, max: *max })
            }
            _ => {}
        };

        debug!(self.logger, "Got arweave file {file}");

        rsp.bytes()
            .await
            .map(|b| b.into())
            .map_err(ArweaveClientError::from)
    }
}

#[async_trait]
pub trait ArweaveResolver: Send + Sync + 'static + Debug {
    async fn get(&self, file: &Base64) -> Result<Vec<u8>, ArweaveClientError>;
    async fn get_with_limit(
        &self,
        file: &Base64,
        limit: &FileSizeLimit,
    ) -> Result<Vec<u8>, ArweaveClientError>;
}

#[derive(Error, Debug)]
pub enum ArweaveClientError {
    #[error("Invalid file URL {0}")]
    InvalidUrl(#[from] url::ParseError),
    #[error("Unable to check the file size")]
    UnableToCheckFileSize,
    #[error("Arweave file is too large. The limit is {max} and file content was {got} bytes")]
    FileTooLarge { got: u64, max: u64 },
    #[error("Unknown error")]
    Unknown(#[from] reqwest::Error),
}

#[cfg(test)]
mod test {
    use serde_derive::Deserialize;

    use crate::{
        components::link_resolver::{ArweaveClient, ArweaveResolver},
        data_source::offchain::Base64,
    };

    // This test ensures that passing txid/filename works when the txid refers to manifest.
    // the actual data seems to have some binary header and footer so these ranges were found
    // by inspecting the data with hexdump.
    #[tokio::test]
    async fn fetch_bundler_url() {
        let url = Base64::from("Rtdn3QWEzM88MPC2dpWyV5waO7Vuz3VwPl_usS2WoHM/DriveManifest.json");
        #[derive(Deserialize, Debug, PartialEq)]
        struct Manifest {
            pub manifest: String,
        }

        let client = ArweaveClient::default();
        let no_header = &client.get(&url).await.unwrap()[1295..320078];
        let content: Manifest = serde_json::from_slice(no_header).unwrap();
        assert_eq!(
            content,
            Manifest {
                manifest: "arweave/paths".to_string(),
            }
        );
    }
}
