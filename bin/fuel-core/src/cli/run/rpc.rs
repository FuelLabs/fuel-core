use clap::{
    Args,
    Subcommand,
};
use fuel_core_types::fuel_types::BlockHeight;
use std::net;

#[derive(Debug, Clone, Args)]
pub struct RpcArgs {
    /// The IP address to bind the RPC service to
    #[clap(long = "rpc_ip", default_value = "127.0.0.1", value_parser, env)]
    pub rpc_ip: net::IpAddr,

    /// The port to bind the RPC service to
    #[clap(long = "rpc_port", default_value = "4001", env)]
    pub rpc_port: u16,

    #[command(subcommand)]
    pub storage_method: Option<StorageMethod>,

    #[clap(long = "api_buffer_size", default_value = "1000", env)]
    pub api_buffer_size: usize,
}

#[derive(Debug, Clone, Subcommand)]
pub enum StorageMethod {
    Local,
    S3 {
        #[clap(long = "bucket", env)]
        bucket: String,
        #[clap(long = "endpoint_url", env)]
        endpoint_url: Option<String>,
        #[clap(long = "requester_pays", env, default_value = "false")]
        requester_pays: bool,
    },
    S3NoPublish {
        #[clap(long = "bucket", env)]
        bucket: String,
        #[clap(long = "endpoint_url", env)]
        endpoint_url: Option<String>,
        #[clap(long = "requester_pays", env, default_value = "false")]
        requester_pays: bool,
    },
}

impl RpcArgs {
    pub fn into_config(self) -> fuel_core_block_aggregator_api::service::Config {
        fuel_core_block_aggregator_api::service::Config {
            addr: net::SocketAddr::new(self.rpc_ip, self.rpc_port),
            sync_from: Some(BlockHeight::from(0)),
            storage_method: self.storage_method.map(Into::into).unwrap_or_default(),
            api_buffer_size: self.api_buffer_size,
        }
    }
}

impl From<StorageMethod> for fuel_core_block_aggregator_api::service::StorageMethod {
    fn from(storage_method: StorageMethod) -> Self {
        match storage_method {
            StorageMethod::Local => {
                fuel_core_block_aggregator_api::service::StorageMethod::Local
            }
            StorageMethod::S3 {
                bucket,
                endpoint_url,
                requester_pays,
            } => fuel_core_block_aggregator_api::service::StorageMethod::S3 {
                bucket,
                endpoint_url,
                requester_pays,
            },
            StorageMethod::S3NoPublish {
                bucket,
                endpoint_url,
                requester_pays,
            } => fuel_core_block_aggregator_api::service::StorageMethod::S3NoPublish {
                bucket,
                endpoint_url,
                requester_pays,
            },
        }
    }
}
