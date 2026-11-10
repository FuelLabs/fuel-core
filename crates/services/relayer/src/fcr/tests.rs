use super::*;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::blockchain::primitives::DaBlockHeight;
use std::{
    collections::BTreeMap,
    io::Read,
    net::TcpListener,
    sync::{
        Arc,
        Mutex,
        atomic::{
            AtomicBool,
            AtomicU64,
            Ordering,
        },
    },
};

#[derive(Default)]
struct Database {
    height: u64,
    activated: bool,
    evidence: BTreeMap<u64, BlockEvidence>,
}
impl RelayerDb for Database {
    fn insert_events(
        &mut self,
        height: &DaBlockHeight,
        _: &[Event],
    ) -> StorageResult<()> {
        self.height = height.0;
        Ok(())
    }
    fn insert_fcr_events(
        &mut self,
        height: &DaBlockHeight,
        events: &[Event],
        evidence: &BlockEvidence,
    ) -> StorageResult<()> {
        self.insert_events(height, events)?;
        self.evidence.insert(height.0, evidence.clone());
        Ok(())
    }
    fn fcr_evidence(
        &self,
        height: &DaBlockHeight,
    ) -> StorageResult<Option<BlockEvidence>> {
        Ok(self.evidence.get(&height.0).cloned())
    }
    fn fcr_activated(&self) -> StorageResult<bool> {
        Ok(self.activated)
    }
    fn activate_fcr(&mut self, _: DaBlockHeight) -> StorageResult<()> {
        self.activated = true;
        Ok(())
    }
    fn get_finalized_da_height(&self) -> Option<DaBlockHeight> {
        Some(self.height.into())
    }
}

#[derive(Default)]
struct View {
    finalized: u64,
    confirmed: u64,
    fail_wrapped: bool,
    fork_route: Option<u64>,
    fork_all: bool,
    forced: bool,
    missing_log: bool,
}
struct Server {
    url: Url,
    view: Arc<Mutex<View>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
fn hash(height: u64) -> B256 {
    B256::from(alloy_primitives::U256::from(height).to_be_bytes::<32>())
}
impl Server {
    fn start(view: View) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        let view = Arc::new(Mutex::new(view));
        let state = view.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let thread = std::thread::spawn(move || {
            while !done.load(Ordering::Acquire) {
                let Ok((mut stream, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut buffer = Vec::new();
                let mut byte = [0; 1];
                while !buffer.ends_with(b"\r\n\r\n") {
                    if stream.read_exact(&mut byte).is_err() {
                        break;
                    }
                    buffer.push(byte[0]);
                }
                let text = String::from_utf8_lossy(&buffer);
                let length: usize = text
                    .lines()
                    .find_map(|line| {
                        line.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|value| value.trim().parse().unwrap())
                    })
                    .unwrap_or(0);
                let route: Vec<_> = text
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .split('/')
                    .collect();
                let wrapped = route.get(1) == Some(&"wrapped");
                let provider: u64 = route.last().unwrap().parse().unwrap_or(0);
                let mut body = vec![0; length];
                stream.read_exact(&mut body).unwrap();
                let request: Value = serde_json::from_slice(&body).unwrap();
                let view = state.lock().unwrap();
                let value = match request["method"].as_str().unwrap() {
                    "eth_chainId" => json!("0x1"),
                    "eth_getBlockByNumber" => {
                        let selector = request["params"][0].as_str().unwrap();
                        let number = match selector {
                            "finalized" => view.finalized,
                            "confirmed" => view.confirmed.saturating_sub(provider),
                            _ => {
                                u64::from_str_radix(selector.trim_start_matches("0x"), 16)
                                    .unwrap()
                            }
                        };
                        if wrapped && view.fail_wrapped {
                            Value::Null
                        } else {
                            let forked = (view.fork_all
                                || view.fork_route == Some(provider))
                                && number == 9;
                            let mut block = json!({"number":format!("0x{number:x}"),"hash":hash(if forked {999} else {number})});
                            if selector != "confirmed" {
                                block["parentHash"] =
                                    json!(hash(number.saturating_sub(1)));
                            }
                            block
                        }
                    }
                    "eth_getLogs" => {
                        let filter = &request["params"][0];
                        let at_nine = filter["blockHash"] == json!(hash(9));
                        if view.forced && !view.missing_log && at_nine {
                            json!([{"address":alloy_primitives::Address::ZERO,"topics":[*ETH_FORCED_TX],"data":"0x","blockNumber":"0x9","blockHash":hash(9),"transactionHash":hash(100),"transactionIndex":"0x0","logIndex":"0x0","removed":false}])
                        } else {
                            json!([])
                        }
                    }
                    other => panic!("Unexpected RPC {other}"),
                };
                let body =
                    serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"result":value}))
                        .unwrap();
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
                stream.write_all(&body).unwrap();
            }
        });
        Self {
            url,
            view,
            stop,
            thread: Some(thread),
        }
    }
    fn driver(&self, path: PathBuf) -> Driver {
        let routes = |prefix: &str| {
            (0..2)
                .map(|n| self.url.join(&format!("{prefix}/{n}")).unwrap())
                .collect()
        };
        Driver::open(
            Config {
                rpc: routes("wrapped"),
                state_path: path,
                chain_id: 1,
                generation: "0".into(),
                max_unchecked_blocks: 128,
                finalized_only: false,
                max_unchecked_seconds: 300,
            },
            routes("direct"),
            &[alloy_primitives::Address::ZERO],
            None,
        )
        .unwrap()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.thread.take().unwrap().join().unwrap();
    }
}
static NEXT: AtomicU64 = AtomicU64::new(0);
fn path() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "fuel-fcr-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path.join("state.json")
}
fn config() -> crate::Config {
    crate::Config {
        eth_v2_listening_contracts: vec![alloy_primitives::Address::ZERO],
        ..Default::default()
    }
}

#[tokio::test]
async fn partial_watermarks_empty_blocks_and_wrapper_failure() {
    let server = Server::start(View {
        finalized: 8,
        confirmed: 11,
        ..Default::default()
    });
    let path = path();
    let mut driver = server.driver(path.clone());
    let mut db = Database {
        height: 8,
        ..Default::default()
    };
    assert_eq!(driver.sync(&mut db, &config()).await.unwrap(), 10);
    assert_eq!(db.height, 10);
    assert!(db.evidence[&9].early);
    assert!(db.evidence[&10].early);
    server.view.lock().unwrap().fail_wrapped = true;
    server.view.lock().unwrap().finalized = 12;
    assert_eq!(driver.sync(&mut db, &config()).await.unwrap(), 12);
    assert_eq!(db.height, 12);
    assert!(!db.evidence[&12].early);
    assert_eq!(driver.status("normal").checked_height, 12);
    drop(driver);
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[tokio::test]
async fn forced_transaction_caps_early_frontier_without_skipping_the_block() {
    let server = Server::start(View {
        finalized: 8,
        confirmed: 11,
        forced: true,
        ..Default::default()
    });
    let path = path();
    let mut driver = server.driver(path.clone());
    let mut db = Database {
        height: 8,
        ..Default::default()
    };
    assert_eq!(driver.sync(&mut db, &config()).await.unwrap(), 8);
    assert_eq!(db.height, 8);
    assert!(db.evidence.is_empty());
    drop(driver);
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[tokio::test]
async fn one_route_holds_but_corroborated_finalized_change_latches_across_restart() {
    let server = Server::start(View {
        finalized: 8,
        confirmed: 11,
        ..Default::default()
    });
    let path = path();
    let mut driver = server.driver(path.clone());
    let mut db = Database {
        height: 8,
        ..Default::default()
    };
    driver.sync(&mut db, &config()).await.unwrap();
    {
        let mut view = server.view.lock().unwrap();
        view.finalized = 11;
        view.fork_route = Some(1);
    }
    assert!(driver.sync(&mut db, &config()).await.is_err());
    assert!(driver.status("held").incident.is_none());
    {
        let mut view = server.view.lock().unwrap();
        view.fork_route = None;
        view.fork_all = true;
    }
    driver.sync(&mut db, &config()).await.unwrap();
    assert!(driver.status("normal").incident.is_some());
    drop(driver);
    let driver = server.driver(path.clone());
    assert!(driver.status("normal").incident.is_some());
    drop(driver);
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[tokio::test]
async fn matching_finalized_header_with_changed_log_digest_is_an_incident() {
    let server = Server::start(View {
        finalized: 8,
        confirmed: 11,
        ..Default::default()
    });
    let path = path();
    let mut driver = server.driver(path.clone());
    let mut db = Database {
        height: 8,
        ..Default::default()
    };
    driver.sync(&mut db, &config()).await.unwrap();
    // Model corrupt retained log evidence with the unchanged Ethereum identity.
    db.evidence.get_mut(&9).unwrap().logs_digest = hash(999);
    server.view.lock().unwrap().finalized = 11;
    driver.sync(&mut db, &config()).await.unwrap();
    assert!(driver.status("normal").incident.is_some());
    drop(driver);
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn activation_missing_state_lock_and_partial_tombstone_fail_closed() {
    let server = Server::start(View::default());
    let path = path();
    let driver = server.driver(path.clone());
    let config = driver.config.clone();
    let routes = driver.direct.clone();
    assert!(
        Driver::open(config, routes, &[alloy_primitives::Address::ZERO], None).is_err()
    );
    fs::write(path.with_extension("incident"), b"").unwrap();
    assert!(driver.status("normal").incident.is_some());
    drop(driver);
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[tokio::test]
async fn offline_recovery_requires_the_actual_da_checkpoint_and_preserves_the_incident_receipt()
 {
    let server = Server::start(View {
        finalized: 8,
        confirmed: 11,
        ..Default::default()
    });
    let path = path();
    let mut driver = server.driver(path.clone());
    let mut db = Database {
        height: 8,
        ..Default::default()
    };
    driver.sync(&mut db, &config()).await.unwrap();
    driver.declare("test-incident").unwrap();
    let mut next = driver.config.clone();
    next.generation = "1".into();
    let direct = driver.direct.clone();
    drop(driver);
    let receipt=Recovery{incident:"test-incident".into(),previous_generation:"0".into(),generation:"1".into(),checked:Header{number:8,hash:hash(8),parent_hash:Some(hash(7))},recovery_record:"Fuel/DA checkpoint verified; Redis generation reset; old signed work invalidated and P2P history isolated".into()};
    assert!(recover_offline(&path, receipt.clone(), 7).is_err());
    assert!(IncidentLatch::new(&path).read().unwrap().is_some());
    recover_offline(&path, receipt, 8).unwrap();
    let driver =
        Driver::open(next, direct, &[alloy_primitives::Address::ZERO], None).unwrap();
    assert!(driver.status("initializing").incident.is_none());
    assert_eq!(driver.status("initializing").checked_height, 8);
    assert!(path.with_extension("incident.1").exists());
    drop(driver);
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn failed_incident_persistence_still_closes_the_live_gate() {
    let server = Server::start(View::default());
    let path = path();
    let mut driver = server.driver(path.clone());
    fs::create_dir(path.with_extension("incident")).unwrap();
    assert!(driver.declare("disk-failure-positive").is_err());
    assert_eq!(
        driver.status("held").incident.as_deref(),
        Some("disk-failure-positive")
    );
    drop(driver);
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[tokio::test]
async fn single_direct_route_contradiction_stays_held_without_a_terminal_incident() {
    let server = Server::start(View {
        finalized: 8,
        confirmed: 11,
        ..Default::default()
    });
    let path = path();
    let mut driver = server.driver(path.clone());
    driver.direct.truncate(1);
    driver.config.rpc.truncate(1);
    let mut db = Database {
        height: 8,
        ..Default::default()
    };
    driver.sync(&mut db, &config()).await.unwrap();
    {
        let mut view = server.view.lock().unwrap();
        view.finalized = 11;
        view.fork_all = true;
    }
    assert!(driver.sync(&mut db, &config()).await.is_err());
    assert!(driver.status("held").incident.is_none());
    drop(driver);
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn later_peer_incident_does_not_replace_the_local_recovery_identifier() {
    let server = Server::start(View::default());
    let path = path();
    let mut driver = server.driver(path.clone());
    driver.declare("first-local").unwrap();
    driver.declare("later-peer").unwrap();
    assert_eq!(
        driver.status("incident").incident.as_deref(),
        Some("first-local")
    );
    assert_eq!(
        IncidentLatch::new(&path).read().unwrap().as_deref(),
        Some("first-local")
    );
    drop(driver);
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
