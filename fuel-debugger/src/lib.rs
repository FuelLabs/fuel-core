use std::io;
use std::net::SocketAddr;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{
    tcp::{OwnedReadHalf, OwnedWriteHalf},
    TcpListener, TcpStream,
};

use fuel_core::debugger::{Command, Response};

pub mod names;

pub struct Listener {
    listener: TcpListener,
}

impl Listener {
    pub fn new(listener: TcpListener) -> Self {
        Self { listener }
    }

    pub async fn accept(&self) -> io::Result<(Client, SocketAddr)> {
        let (mut socket, addr) = self.listener.accept().await?;

        let (reader, writer) = socket.into_split();
        let reader = BufReader::new(reader);

        Ok((Client { reader, writer }, addr))
    }
}

pub struct Client {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
}

impl Client {
    pub async fn cmd(&mut self, cmd: &Command) -> io::Result<Response> {
        let mut v = serde_json::to_string(cmd).expect("Serialization failed");
        v.push('\n');
        self.writer
            .write(v.as_bytes())
            .await
            .expect("Sending failed");

        let mut line = String::new();
        let _ = self.reader.read_line(&mut line).await?;
        let resp: Response = serde_json::from_str(&line).expect("Invalid JSON from the debugger");
        Ok(resp)
    }
}
