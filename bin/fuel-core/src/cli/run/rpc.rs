use clap::Args;
use std::net;

#[derive(Debug, Clone, Args)]
pub struct RpcArgs {
    /// The IP address to bind the RPC service to
    #[clap(long = "ip", default_value = "127.0.0.1", value_parser, env)]
    pub ip: net::IpAddr,

    /// The port to bind the RPC service to
    #[clap(long = "port", default_value = "4000", env)]
    pub port: u16,
}

impl RpcArgs {
    pub fn into_config(self) -> fuel_block_aggregator_api::integration::Config {
        fuel_block_aggregator_api::integration::Config {
            addr: net::SocketAddr::new(self.ip, self.port),
        }
    }
}
