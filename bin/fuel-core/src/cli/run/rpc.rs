use clap::{
    Args,
    ValueEnum,
};
use fuel_core_types::fuel_types::BlockHeight;
use std::{
    collections::HashMap,
    net,
};

fn parse_public_http_headers(entries: &[String]) -> HashMap<String, String> {
    entries
        .iter()
        .filter_map(|e| {
            e.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
        })
        .collect()
}

#[derive(Debug, Clone, Args)]
pub struct RpcArgs {
    /// The IP address to bind the RPC service to
    #[clap(long = "rpc-ip", default_value = "127.0.0.1", value_parser, env)]
    pub rpc_ip: net::IpAddr,

    /// The port to bind the RPC service to
    #[clap(long = "rpc-port", default_value = "4001", env)]
    pub rpc_port: u16,

    #[clap(long = "rpc-storage-method", value_enum, default_value = "local", env)]
    pub rpc_storage_method: StorageMethod,

    #[clap(long = "rpc-s3-bucket", env)]
    #[clap(required_if_eq("rpc_storage_method", "s3"))]
    #[clap(required_if_eq("rpc_storage_method", "s3-no-publish"))]
    pub rpc_s3_bucket: Option<String>,

    #[clap(long = "rpc-endpoint-url", env)]
    pub rpc_endpoint_url: Option<String>,

    #[clap(long = "rpc-s3-requester-pays", env, default_value = "false")]
    pub rpc_s3_requester_pays: bool,

    /// Public HTTP(S) base URL for block objects (e.g. CDN in front of the S3 bucket). When set,
    /// gRPC returns `RemoteHttpEndpoint` URLs (`{base}/{s3-key}`) instead of `RemoteS3Bucket`; uploads still use S3.
    #[clap(long = "rpc-public-block-http-url", env)]
    pub rpc_public_block_http_url: Option<String>,

    /// Optional extra HTTP headers for clients fetching blocks from `--rpc-public-block-http-url` (format `NAME=value`, repeatable).
    #[clap(long = "rpc-public-block-http-header", env, value_name = "NAME=value")]
    pub rpc_public_block_http_headers: Vec<String>,

    #[clap(long = "rpc-api_buffer_size", default_value = "1000", env)]
    pub rpc_api_buffer_size: usize,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum StorageMethod {
    Local,
    S3,
    #[clap(name = "s3-no-publish")]
    S3NoPublish,
}

impl RpcArgs {
    pub fn into_config(self) -> fuel_core_block_aggregator_api::service::Config {
        let public_headers = parse_public_http_headers(&self.rpc_public_block_http_headers);
        let storage_method = match self.rpc_storage_method {
            StorageMethod::Local => {
                fuel_core_block_aggregator_api::service::StorageMethod::Local
            }
            StorageMethod::S3 => {
                fuel_core_block_aggregator_api::service::StorageMethod::S3 {
                    bucket: self
                        .rpc_s3_bucket
                        .expect("storage_method=s3 requires --bucket"),
                    endpoint_url: self.rpc_endpoint_url,
                    requester_pays: self.rpc_s3_requester_pays,
                    public_block_http_url: self.rpc_public_block_http_url,
                    public_block_http_headers: public_headers.clone(),
                }
            }
            StorageMethod::S3NoPublish => {
                fuel_core_block_aggregator_api::service::StorageMethod::S3NoPublish {
                    bucket: self
                        .rpc_s3_bucket
                        .expect("storage_method=s3-no-publish requires --bucket"),
                    endpoint_url: self.rpc_endpoint_url,
                    requester_pays: self.rpc_s3_requester_pays,
                    public_block_http_url: self.rpc_public_block_http_url,
                    public_block_http_headers: public_headers,
                }
            }
        };

        fuel_core_block_aggregator_api::service::Config {
            addr: net::SocketAddr::new(self.rpc_ip, self.rpc_port),
            sync_from: Some(BlockHeight::from(0)),
            storage_method,
            api_buffer_size: self.rpc_api_buffer_size,
        }
    }
}
