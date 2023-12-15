use crate::prelude::CheapClone;
use anyhow::anyhow;
use anyhow::Error;
use bytes::Bytes;
use cid::Cid;
use futures03::Stream;
use http::header::CONTENT_LENGTH;
use http::Uri;
use reqwest::multipart;
use serde::Deserialize;
use std::fmt::Display;
use std::time::Duration;
use std::{str::FromStr, sync::Arc};

/// Represents a file on Ipfs. This file can be the CID or a path within a folder CID.
/// The path cannot have a prefix (ie CID/hello.json would be cid: CID path: "hello.json")
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash)]
pub struct CidFile {
    pub cid: Cid,
    pub path: Option<String>,
}

impl Display for CidFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self.path {
            Some(ref f) => format!("{}/{}", self.cid, f),
            None => self.cid.to_string(),
        };
        f.write_str(&str)
    }
}

impl CidFile {
    pub fn to_bytes(&self) -> Vec<u8> {
        self.to_string().as_bytes().to_vec()
    }
}

impl TryFrom<crate::data::store::scalar::Bytes> for CidFile {
    type Error = anyhow::Error;

    fn try_from(value: crate::data::store::scalar::Bytes) -> Result<Self, Self::Error> {
        let str = String::from_utf8(value.to_vec())?;

        Self::from_str(&str)
    }
}

/// The string should not have a prefix and only one slash after the CID is removed, everything
/// else is considered a file path. If this is malformed, it will fail to find the file.
impl FromStr for CidFile {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(anyhow!("cid can't be empty"));
        }

        let cid_str: String = s.chars().take_while(|c| *c != '/').collect();
        let cid = Cid::from_str(&cid_str)?;

        // if cid was the only content or if it's just slash terminated.
        if cid_str.len() == s.len() || s.len() + 1 == cid_str.len() {
            return Ok(CidFile { cid, path: None });
        }

        let file: String = s[cid_str.len() + 1..].to_string();
        let path = if file.is_empty() { None } else { Some(file) };

        Ok(CidFile { cid, path })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StatApi {
    Block,
    Files,
}

impl StatApi {
    fn route(&self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Files => "files",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct BlockStatResponse {
    size: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct FilesStatResponse {
    cumulative_size: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AddResponse {
    pub name: String,
    pub hash: String,
    pub size: String,
}

/// Reference type, clones will share the connection pool.
#[derive(Clone)]
pub struct IpfsClient {
    base: Arc<Uri>,
    // reqwest::Client doesn't need to be `Arc` because it has one internally
    // already.
    client: reqwest::Client,
}

impl CheapClone for IpfsClient {
    fn cheap_clone(&self) -> Self {
        IpfsClient {
            base: self.base.cheap_clone(),
            client: self.client.cheap_clone(),
        }
    }
}

impl IpfsClient {
    pub fn new(base: &str) -> Result<Self, Error> {
        Ok(IpfsClient {
            client: reqwest::Client::new(),
            base: Arc::new(Uri::from_str(base)?),
        })
    }

    pub fn localhost() -> Self {
        IpfsClient {
            client: reqwest::Client::new(),
            base: Arc::new(Uri::from_str("http://localhost:5001").unwrap()),
        }
    }

    /// Calls stat for the given API route, and returns the total size of the object.
    pub async fn stat_size(
        &self,
        api: StatApi,
        mut cid: String,
        timeout: Duration,
    ) -> Result<u64, reqwest::Error> {
        let route = format!("{}/stat", api.route());
        if api == StatApi::Files {
            // files/stat requires a leading `/ipfs/`.
            cid = format!("/ipfs/{}", cid);
        }
        let url = self.url(&route, &cid);
        let res = self.call(url, None, Some(timeout)).await?;
        match api {
            StatApi::Files => Ok(res.json::<FilesStatResponse>().await?.cumulative_size),
            StatApi::Block => Ok(res.json::<BlockStatResponse>().await?.size),
        }
    }

    /// Download the entire contents.
    pub async fn cat_all(&self, cid: &str, timeout: Duration) -> Result<Bytes, reqwest::Error> {
        self.call(self.url("cat", cid), None, Some(timeout))
            .await?
            .bytes()
            .await
    }

    pub async fn cat(
        &self,
        cid: &str,
        timeout: Option<Duration>,
    ) -> Result<impl Stream<Item = Result<Bytes, reqwest::Error>>, reqwest::Error> {
        Ok(self
            .call(self.url("cat", cid), None, timeout)
            .await?
            .bytes_stream())
    }

    pub async fn get_block(&self, cid: String) -> Result<Bytes, reqwest::Error> {
        let form = multipart::Form::new().part("arg", multipart::Part::text(cid));
        self.call(format!("{}api/v0/block/get", self.base), Some(form), None)
            .await?
            .bytes()
            .await
    }

    pub async fn test(&self) -> Result<(), reqwest::Error> {
        self.call(format!("{}api/v0/version", self.base), None, None)
            .await
            .map(|_| ())
    }

    pub async fn add(&self, data: Vec<u8>) -> Result<AddResponse, reqwest::Error> {
        let form = multipart::Form::new().part("path", multipart::Part::bytes(data));

        self.call(format!("{}api/v0/add", self.base), Some(form), None)
            .await?
            .json()
            .await
    }

    fn url(&self, route: &str, arg: &str) -> String {
        // URL security: We control the base and the route, user-supplied input goes only into the
        // query parameters.
        format!("{}api/v0/{}?arg={}", self.base, route, arg)
    }

    async fn call(
        &self,
        url: String,
        form: Option<multipart::Form>,
        timeout: Option<Duration>,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let mut req = self.client.post(&url);
        if let Some(form) = form {
            req = req.multipart(form);
        } else {
            // Some servers require `content-length` even for an empty body.
            req = req.header(CONTENT_LENGTH, 0);
        }

        if let Some(timeout) = timeout {
            req = req.timeout(timeout)
        }

        req.send()
            .await
            .map(|res| res.error_for_status())
            .and_then(|x| x)
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use anyhow::anyhow;
    use cid::Cid;

    use crate::ipfs_client::CidFile;

    #[test]
    fn test_cid_parsing() {
        let cid_str = "bafyreibjo4xmgaevkgud7mbifn3dzp4v4lyaui4yvqp3f2bqwtxcjrdqg4";
        let cid = Cid::from_str(cid_str).unwrap();

        struct Case<'a> {
            name: &'a str,
            input: String,
            path: String,
            expected: Result<CidFile, anyhow::Error>,
        }

        let cases = vec![
            Case {
                name: "correct no slashes, no file",
                input: cid_str.to_string(),
                path: cid_str.to_string(),
                expected: Ok(CidFile { cid, path: None }),
            },
            Case {
                name: "correct with file path",
                input: format!("{}/file.json", cid),
                path: format!("{}/file.json", cid_str),
                expected: Ok(CidFile {
                    cid,
                    path: Some("file.json".into()),
                }),
            },
            Case {
                name: "correct cid with trailing slash",
                input: format!("{}/", cid),
                path: format!("{}", cid),
                expected: Ok(CidFile { cid, path: None }),
            },
            Case {
                name: "incorrect, empty",
                input: "".to_string(),
                path: "".to_string(),
                expected: Err(anyhow!("cid can't be empty")),
            },
            Case {
                name: "correct, two slahes",
                input: format!("{}//", cid),
                path: format!("{}//", cid),
                expected: Ok(CidFile {
                    cid,
                    path: Some("/".into()),
                }),
            },
            Case {
                name: "incorrect, leading slahes",
                input: format!("/ipfs/{}/file.json", cid),
                path: "".to_string(),
                expected: Err(anyhow!("Input too short")),
            },
            Case {
                name: "correct syntax, invalid CID",
                input: "notacid/file.json".to_string(),
                path: "".to_string(),
                expected: Err(anyhow!("Failed to parse multihash")),
            },
        ];

        for case in cases {
            let f = CidFile::from_str(&case.input);

            match case.expected {
                Ok(cid_file) => {
                    assert!(f.is_ok(), "case: {}", case.name);
                    let f = f.unwrap();
                    assert_eq!(f, cid_file, "case: {}", case.name);
                    assert_eq!(f.to_string(), case.path, "case: {}", case.name);
                }
                Err(err) => assert_eq!(
                    f.unwrap_err().to_string(),
                    err.to_string(),
                    "case: {}",
                    case.name
                ),
            }
        }
    }
}
