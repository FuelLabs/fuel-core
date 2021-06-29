use structopt::StructOpt;
use tracing_subscriber::filter::EnvFilter;

use std::{env, io, net};

#[derive(StructOpt, Debug)]
pub struct Opt {
    #[structopt(long = "ip", default_value = "0.0.0.0", parse(try_from_str))]
    pub ip: net::IpAddr,

    #[structopt(long = "port", default_value = "4000")]
    pub port: u16,
}

impl Opt {
    pub fn exec(self) -> io::Result<net::SocketAddr> {
        let filter = match env::var_os("RUST_LOG") {
            Some(_) => EnvFilter::try_from_default_env().expect("Invalid `RUST_LOG` provided"),
            None => EnvFilter::new("info"),
        };

        tracing_subscriber::fmt::Subscriber::builder()
            .with_writer(std::io::stderr)
            .with_env_filter(filter)
            .init();

        let Opt { ip, port } = self;

        let addr = net::SocketAddr::new(ip, port);

        Ok(addr)
    }
}
