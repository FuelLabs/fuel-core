use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};
use tracing::{debug, trace, warn};

use crate::redis_server::bind_unused_port;

#[derive(Debug, Clone)]
pub enum ProxyMode {
    /// Pass-through: forward all traffic normally
    Normal,
    /// Drop all connections immediately (simulates network partition)
    DropAll,
    /// Delay each direction before forwarding
    Latency(Duration),
    /// Kill the connection after forwarding N bytes
    CloseAfterBytes(usize),
}

pub struct TcpProxy {
    listen_port: u16,
    mode: Arc<Mutex<ProxyMode>>,
    _task: JoinHandle<()>,
}

impl TcpProxy {
    pub async fn start(target_addr: String) -> Self {
        let listen_port = bind_unused_port();
        let mode = Arc::new(Mutex::new(ProxyMode::Normal));

        let listener = TcpListener::bind(format!("127.0.0.1:{listen_port}"))
            .await
            .unwrap_or_else(|e| panic!("Failed to bind proxy on port {listen_port}: {e}"));

        let task_mode = mode.clone();
        let task_target = target_addr.clone();
        let task = tokio::spawn(async move {
            Self::accept_loop(listener, task_target, task_mode).await;
        });

        Self {
            listen_port,
            mode,
            _task: task,
        }
    }

    pub fn set_mode(&self, new_mode: ProxyMode) {
        *self.mode.lock().unwrap() = new_mode;
    }

    pub fn listen_url(&self) -> String {
        format!("redis://127.0.0.1:{}/", self.listen_port)
    }

    pub fn listen_port(&self) -> u16 {
        self.listen_port
    }

    async fn accept_loop(
        listener: TcpListener,
        target_addr: String,
        mode: Arc<Mutex<ProxyMode>>,
    ) {
        loop {
            let (client, addr) = match listener.accept().await {
                Ok(conn) => conn,
                Err(e) => {
                    warn!("Proxy accept error: {e}");
                    continue;
                }
            };

            let current_mode = mode.lock().unwrap().clone();
            let target = target_addr.clone();
            let conn_mode = mode.clone();

            trace!("Proxy accepted connection from {addr}, mode: {current_mode:?}");

            tokio::spawn(async move {
                if let Err(e) =
                    Self::handle_connection(client, &target, conn_mode).await
                {
                    debug!("Proxy connection ended: {e}");
                }
            });
        }
    }

    async fn handle_connection(
        mut client: TcpStream,
        target_addr: &str,
        mode: Arc<Mutex<ProxyMode>>,
    ) -> anyhow::Result<()> {
        let current_mode = mode.lock().unwrap().clone();

        match current_mode {
            ProxyMode::DropAll => {
                // Close immediately
                drop(client);
                return Ok(());
            }
            ProxyMode::Normal => {
                let mut upstream =
                    TcpStream::connect(target_addr).await?;
                tokio::io::copy_bidirectional(&mut client, &mut upstream)
                    .await?;
            }
            ProxyMode::Latency(delay) => {
                let mut upstream =
                    TcpStream::connect(target_addr).await?;
                Self::relay_with_latency(
                    &mut client,
                    &mut upstream,
                    delay,
                    mode,
                )
                .await?;
            }
            ProxyMode::CloseAfterBytes(limit) => {
                let mut upstream =
                    TcpStream::connect(target_addr).await?;
                Self::relay_with_byte_limit(
                    &mut client,
                    &mut upstream,
                    limit,
                )
                .await?;
            }
        }

        Ok(())
    }

    async fn relay_with_latency(
        client: &mut TcpStream,
        upstream: &mut TcpStream,
        delay: Duration,
        mode: Arc<Mutex<ProxyMode>>,
    ) -> anyhow::Result<()> {
        let (mut client_read, mut client_write) = client.split();
        let (mut upstream_read, mut upstream_write) = upstream.split();

        let mode_a = mode.clone();
        let mode_b = mode;

        let client_to_upstream = async move {
            let mut buf = [0u8; 4096];
            loop {
                let current = mode_a.lock().unwrap().clone();
                if matches!(current, ProxyMode::DropAll) {
                    return anyhow::Ok(());
                }
                let n = match tokio::time::timeout(
                    Duration::from_secs(30),
                    client_read.read(&mut buf),
                )
                .await
                {
                    Ok(Ok(0)) | Err(_) => return Ok(()),
                    Ok(Ok(n)) => n,
                    Ok(Err(e)) => return Err(e.into()),
                };
                tokio::time::sleep(delay).await;
                upstream_write.write_all(&buf[..n]).await?;
            }
        };

        let upstream_to_client = async move {
            let mut buf = [0u8; 4096];
            loop {
                let current = mode_b.lock().unwrap().clone();
                if matches!(current, ProxyMode::DropAll) {
                    return anyhow::Ok(());
                }
                let n = match tokio::time::timeout(
                    Duration::from_secs(30),
                    upstream_read.read(&mut buf),
                )
                .await
                {
                    Ok(Ok(0)) | Err(_) => return Ok(()),
                    Ok(Ok(n)) => n,
                    Ok(Err(e)) => return Err(e.into()),
                };
                tokio::time::sleep(delay).await;
                client_write.write_all(&buf[..n]).await?;
            }
        };

        tokio::select! {
            r = client_to_upstream => r?,
            r = upstream_to_client => r?,
        }

        Ok(())
    }

    async fn relay_with_byte_limit(
        client: &mut TcpStream,
        upstream: &mut TcpStream,
        limit: usize,
    ) -> anyhow::Result<()> {
        let (mut client_read, mut client_write) = client.split();
        let (mut upstream_read, mut upstream_write) = upstream.split();

        let mut total_bytes = 0usize;

        let client_to_upstream = async {
            let mut buf = [0u8; 4096];
            loop {
                let n = match client_read.read(&mut buf).await {
                    Ok(0) => return anyhow::Ok(()),
                    Ok(n) => n,
                    Err(e) => return Err(e.into()),
                };
                total_bytes += n;
                if total_bytes >= limit {
                    return Ok(());
                }
                upstream_write.write_all(&buf[..n]).await?;
            }
        };

        let upstream_to_client = async {
            let mut buf = [0u8; 4096];
            loop {
                let n = match upstream_read.read(&mut buf).await {
                    Ok(0) => return anyhow::Ok(()),
                    Ok(n) => n,
                    Err(e) => return Err(e.into()),
                };
                client_write.write_all(&buf[..n]).await?;
            }
        };

        tokio::select! {
            r = client_to_upstream => r?,
            r = upstream_to_client => r?,
        }

        Ok(())
    }
}
