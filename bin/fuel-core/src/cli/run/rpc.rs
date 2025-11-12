use clap::Args;
use std::net;

#[derive(Debug, Clone, Args)]
pub struct RpcArgs {
    /// The IP address to bind the RPC service to
    #[clap(long = "rpc_ip", default_value = "127.0.0.1", value_parser, env)]
    pub rpc_ip: net::IpAddr,

    /// The port to bind the RPC service to
    #[clap(long = "rpc_port", default_value = "4001", env)]
    pub rpc_port: u16,
}

impl RpcArgs {
    pub fn into_config(self) -> fuel_core_block_aggregator_api::integration::Config {
        fuel_core_block_aggregator_api::integration::Config {
            addr: net::SocketAddr::new(self.rpc_ip, self.rpc_port),
        }
    }
}
