use anyhow::{anyhow, Error};
use bytes::Bytes;
use futures::future::BoxFuture;
use graph::{
    ipfs_client::{CidFile, IpfsClient, StatApi},
    prelude::CheapClone,
};
use std::time::Duration;
use tower::{buffer::Buffer, ServiceBuilder, ServiceExt};

const CLOUDFLARE_TIMEOUT: u16 = 524;
const GATEWAY_TIMEOUT: u16 = 504;

pub type IpfsService = Buffer<CidFile, BoxFuture<'static, Result<Option<Bytes>, Error>>>;

pub fn ipfs_service(
    client: IpfsClient,
    max_file_size: u64,
    timeout: Duration,
    rate_limit: u16,
) -> IpfsService {
    let ipfs = IpfsServiceInner {
        client,
        max_file_size,
        timeout,
    };

    let svc = ServiceBuilder::new()
        .rate_limit(rate_limit.into(), Duration::from_secs(1))
        .service_fn(move |req| ipfs.cheap_clone().call_inner(req))
        .boxed();

    // The `Buffer` makes it so the rate limit is shared among clones.
    // Make it unbounded to avoid any risk of starvation.
    Buffer::new(svc, u32::MAX as usize)
}

#[derive(Clone)]
struct IpfsServiceInner {
    client: IpfsClient,
    max_file_size: u64,
    timeout: Duration,
}

impl CheapClone for IpfsServiceInner {
    fn cheap_clone(&self) -> Self {
        Self {
            client: self.client.cheap_clone(),
            max_file_size: self.max_file_size,
            timeout: self.timeout,
        }
    }
}

impl IpfsServiceInner {
    async fn call_inner(self, req: CidFile) -> Result<Option<Bytes>, Error> {
        let CidFile { cid, path } = req;
        let multihash = cid.hash().code();
        if !SAFE_MULTIHASHES.contains(&multihash) {
            return Err(anyhow!("CID multihash {} is not allowed", multihash));
        }

        let cid_str = match path {
            Some(path) => format!("{}/{}", cid, path),
            None => cid.to_string(),
        };

        let size = match self
            .client
            .stat_size(StatApi::Files, cid_str.clone(), self.timeout)
            .await
        {
            Ok(size) => size,
            Err(e) => match e.status().map(|e| e.as_u16()) {
                Some(GATEWAY_TIMEOUT) | Some(CLOUDFLARE_TIMEOUT) => return Ok(None),
                _ if e.is_timeout() => return Ok(None),
                _ => return Err(e.into()),
            },
        };

        if size > self.max_file_size {
            return Err(anyhow!(
                "IPFS file {} is too large. It can be at most {} bytes but is {} bytes",
                cid_str,
                self.max_file_size,
                size
            ));
        }

        Ok(self
            .client
            .cat_all(&cid_str, self.timeout)
            .await
            .map(Some)?)
    }
}

// Multihashes that are collision resistant. This is not complete but covers the commonly used ones.
// Code table: https://github.com/multiformats/multicodec/blob/master/table.csv
// rust-multihash code enum: https://github.com/multiformats/rust-multihash/blob/master/src/multihash_impl.rs
const SAFE_MULTIHASHES: [u64; 15] = [
    0x0,    // Identity
    0x12,   // SHA2-256 (32-byte hash size)
    0x13,   // SHA2-512 (64-byte hash size)
    0x17,   // SHA3-224 (28-byte hash size)
    0x16,   // SHA3-256 (32-byte hash size)
    0x15,   // SHA3-384 (48-byte hash size)
    0x14,   // SHA3-512 (64-byte hash size)
    0x1a,   // Keccak-224 (28-byte hash size)
    0x1b,   // Keccak-256 (32-byte hash size)
    0x1c,   // Keccak-384 (48-byte hash size)
    0x1d,   // Keccak-512 (64-byte hash size)
    0xb220, // BLAKE2b-256 (32-byte hash size)
    0xb240, // BLAKE2b-512 (64-byte hash size)
    0xb260, // BLAKE2s-256 (32-byte hash size)
    0x1e,   // BLAKE3-256 (32-byte hash size)
];

#[cfg(test)]
mod test {
    use ipfs::IpfsApi;
    use ipfs_api as ipfs;
    use std::{fs, str::FromStr, time::Duration};
    use tower::ServiceExt;

    use cid::Cid;
    use graph::{
        components::link_resolver::{ArweaveClient, ArweaveResolver},
        data::value::Word,
        ipfs_client::IpfsClient,
        tokio,
    };

    use uuid::Uuid;

    #[tokio::test]
    async fn cat_file_in_folder() {
        let path = "./tests/fixtures/ipfs_folder";
        let uid = Uuid::new_v4().to_string();
        fs::write(format!("{}/random.txt", path), &uid).unwrap();

        let cl: ipfs::IpfsClient = ipfs::IpfsClient::default();

        let rsp = cl.add_path(path).await.unwrap();

        let ipfs_folder = rsp.iter().find(|rsp| rsp.name == "ipfs_folder").unwrap();

        let local = IpfsClient::localhost();
        let cid = Cid::from_str(&ipfs_folder.hash).unwrap();
        let file = "random.txt".to_string();

        let svc = super::ipfs_service(local, 100000, Duration::from_secs(5), 10);

        let content = svc
            .oneshot(super::CidFile {
                cid,
                path: Some(file),
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(content.to_vec(), uid.as_bytes().to_vec());
    }

    #[tokio::test]
    async fn arweave_get() {
        const ID: &str = "8APeQ5lW0-csTcBaGdPBDLAL2ci2AT9pTn2tppGPU_8";

        let cl = ArweaveClient::default();
        let body = cl.get(&Word::from(ID)).await.unwrap();
        let body = String::from_utf8(body).unwrap();

        let expected = r#"
            {"name":"Arloader NFT #1","description":"Super dope, one of a kind NFT","collection":{"name":"Arloader NFT","family":"We AR"},"attributes":[{"trait_type":"cx","value":-0.4042198883730073},{"trait_type":"cy","value":0.5641681708263335},{"trait_type":"iters","value":44}],"properties":{"category":"image","files":[{"uri":"https://arweave.net/7gWCr96zc0QQCXOsn5Vk9ROVGFbMaA9-cYpzZI8ZMDs","type":"image/png"},{"uri":"https://arweave.net/URwQtoqrbYlc5183STNy3ZPwSCRY4o8goaF7MJay3xY/1.png","type":"image/png"}]},"image":"https://arweave.net/URwQtoqrbYlc5183STNy3ZPwSCRY4o8goaF7MJay3xY/1.png"}
        "#.trim_start().trim_end();
        assert_eq!(expected, body);
    }
}
