use structopt::StructOpt;
use tracing_subscriber::filter::EnvFilter;

use std::{io, net};

#[derive(StructOpt, Debug)]
pub struct Opt {
    #[structopt(long = "ip", default_value = "0.0.0.0", parse(try_from_str))]
    pub ip: net::IpAddr,

    #[structopt(long = "port", default_value = "4000")]
    pub port: u16,
}

impl Opt {
    pub fn exec(self) -> io::Result<net::SocketAddr> {
        let filter = EnvFilter::from_default_env();
        tracing_subscriber::fmt::Subscriber::builder()
            .with_writer(std::io::stderr)
            .with_env_filter(filter)
            .init();

        let Opt { ip, port } = self;

        let addr = net::SocketAddr::new(ip, port);

        Ok(addr)
    }
}
