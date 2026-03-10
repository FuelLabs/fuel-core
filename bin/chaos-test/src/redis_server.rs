use std::{
    net::{SocketAddrV4, TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use tempfile::TempDir;

pub struct RedisTestServer {
    child: Option<Child>,
    port: u16,
    /// Persistent data directory for AOF — survives kill/restart
    data_dir: TempDir,
}


impl RedisTestServer {
    pub fn spawn() -> Self {
        let mut server = Self::new_stopped();
        server.start();
        server
    }

    pub fn new_stopped() -> Self {
        let port = bind_unused_port();
        let data_dir = TempDir::new().unwrap_or_else(|e| {
            panic!("Failed to create temp dir for Redis on port {port}: {e}")
        });
        Self {
            child: None,
            port,
            data_dir,
        }
    }

    pub fn start(&mut self) {
        if self.child.is_some() {
            return;
        }
        let child = spawn_redis_server(self.port, self.data_dir.path());
        wait_for_redis_ready(self.port);
        self.child = Some(child);
    }

    pub fn stop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.child = None;
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn is_running(&self) -> bool {
        self.child.is_some()
    }
}

impl Drop for RedisTestServer {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub fn bind_unused_port() -> u16 {
    let socket =
        TcpListener::bind(SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, 0))
            .expect("Should bind an ephemeral port");
    let port = socket.local_addr().expect("Should get local addr").port();
    drop(socket);
    port
}

fn spawn_redis_server(port: u16, data_dir: &std::path::Path) -> Child {
    Command::new("redis-server")
        .arg("--port")
        .arg(port.to_string())
        .arg("--save")
        .arg("")
        .arg("--appendonly")
        .arg("yes")
        .arg("--appendfsync")
        .arg("everysec")
        .arg("--dir")
        .arg(data_dir.to_str().expect("data dir should be valid UTF-8"))
        .arg("--bind")
        .arg("127.0.0.1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("redis-server must be installed for this test")
}

fn wait_for_redis_ready(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }
    panic!("redis-server did not become ready in time on port {port}");
}
