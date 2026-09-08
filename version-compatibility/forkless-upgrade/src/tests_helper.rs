#![allow(dead_code)]
use anyhow::Context;
use genesis_fuel_core_bin::FuelService as GenesisFuelService;
use genesis_fuel_core_client::client::FuelClient as GenesisClient;
use genesis_fuel_core_services::Service as _;
use latest_fuel_core_bin::FuelService as LatestFuelService;
use latest_fuel_core_client::client::FuelClient as LatestClient;
use libp2p::PeerId;
use rand::{
    Rng,
    prelude::StdRng,
};
use std::{
    io::{
        BufRead,
        BufReader,
    },
    net::SocketAddr,
    path::PathBuf,
    process::{
        Child,
        Command,
        Stdio,
    },
    str::FromStr,
    time::Duration,
};
use version_44_fuel_core_client::client::FuelClient as Version44Client;
use version_44_fuel_core_type::{
    fuel_crypto::{
        SecretKey,
        fuel_types::ChainId,
    },
    fuel_tx::{
        Input,
        Signable,
        Transaction,
        Upgrade,
        UpgradePurpose,
        Upload,
        UploadSubsection,
        Witness,
        policies::Policies,
    },
};

// Awful version compatibility hack.
// `$bin_crate::cli::run::get_service` is async in the later versions of fuel-core-bin.
macro_rules! maybe_await {
    (true, $expr:expr_2021) => {
        $expr.await
    };
    (false, $expr:expr_2021) => {
        $expr
    };
}

macro_rules! define_core_driver {
    ($bin_crate:ident, $service:ident, $client:ident, $name:ident, $bin_crate_get_service_is_async:tt) => {
        pub struct $name {
            /// This must be before the _db_dir as the drop order matters here.
            pub node: $service,
            pub _db_dir: tempfile::TempDir,
            pub client: $client,
        }

        impl $name {
            pub async fn spawn(extra_args: &[&str]) -> anyhow::Result<Self> {
                use tempfile::tempdir;
                let db_dir = tempdir()?;

                Self::spawn_with_directory(db_dir, extra_args).await
            }
            pub async fn spawn_with_directory(
                db_dir: tempfile::TempDir,
                extra_args: &[&str],
            ) -> anyhow::Result<Self> {
                use clap::Parser;

                let mut args = vec![
                    "_IGNORED_",
                    "--db-path",
                    db_dir.path().to_str().unwrap(),
                    "--port",
                    "0",
                ];
                args.extend(extra_args);

                let node = maybe_await!(
                    $bin_crate_get_service_is_async,
                    $bin_crate::cli::run::get_service(
                        $bin_crate::cli::run::Command::parse_from(args),
                    )
                )?;

                node.start_and_await().await?;

                let client = $client::from(node.shared.graph_ql.bound_address);
                Ok(Self {
                    node,
                    _db_dir: db_dir,
                    client,
                })
            }
        }
    };
}

define_core_driver!(
    genesis_fuel_core_bin,
    GenesisFuelService,
    GenesisClient,
    GenesisFuelCoreDriver,
    false
);

/// The historical binary has its own dependency graph and lockfile.
pub struct Version44FuelCoreDriver {
    // Reap the child before removing its database.
    process: HistoricalProcess,
    pub _db_dir: tempfile::TempDir,
    pub client: Version44Client,
}

struct HistoricalProcess {
    child: Child,
    log: tempfile::NamedTempFile,
}

impl HistoricalProcess {
    fn ensure_running(&mut self) -> anyhow::Result<()> {
        if let Some(status) = self.child.try_wait()? {
            anyhow::bail!("Historical node exited unexpectedly: {status}");
        }
        Ok(())
    }

    fn logs(&self) -> String {
        std::fs::read_to_string(self.log.path()).unwrap_or_else(|error| {
            format!("Unable to read historical node logs: {error}")
        })
    }

    async fn client(&mut self) -> anyhow::Result<Version44Client> {
        // Let the OS allocate the port. Reading the pinned release's structured
        // startup log avoids the race inherent in reserving and releasing a port.
        let mut reader = BufReader::new(self.log.reopen()?);
        let mut line = String::new();
        let client = 'endpoint: loop {
            self.ensure_running()?;
            while reader.read_line(&mut line)? != 0 {
                if !line.ends_with('\n') {
                    break;
                }
                if let Ok(record) = serde_json::from_str::<serde_json::Value>(&line)
                    && let Some(address) =
                        record["fields"]["message"].as_str().and_then(|message| {
                            message.strip_prefix("Binding GraphQL provider to ")
                        })
                {
                    break 'endpoint Version44Client::from(address.parse::<SocketAddr>()?);
                }
                line.clear();
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        };
        loop {
            self.ensure_running()?;
            if client.health().await.unwrap_or(false) {
                let version = client.node_info().await?.node_version;
                anyhow::ensure!(
                    version == "0.44.0",
                    "Expected historical node 0.44.0, got {version}"
                );
                self.ensure_running()?;
                return Ok(client);
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}

impl Drop for HistoricalProcess {
    fn drop(&mut self) {
        // Drop also runs during unwinding and cancelled async startup. Always
        // reap before TempDir cleanup; a detached node must never outlive a test.
        let _ = self.child.kill();
        let _ = self.child.wait();
        if std::thread::panicking() {
            eprintln!("Historical node logs:\n{}", self.logs());
        }
    }
}

impl Version44FuelCoreDriver {
    pub async fn spawn(extra_args: &[&str]) -> anyhow::Result<Self> {
        Self::spawn_with_directory(tempfile::tempdir()?, extra_args).await
    }

    pub async fn spawn_with_directory(
        db_dir: tempfile::TempDir,
        extra_args: &[&str],
    ) -> anyhow::Result<Self> {
        let executable = std::env::var_os("FUEL_CORE_V44_BIN")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../target/historical/v0.44.0/bin/fuel-core")
                    .with_extension(std::env::consts::EXE_EXTENSION)
            });
        let log = tempfile::NamedTempFile::new()?;
        let child = Command::new(&executable)
            .arg("run")
            .arg("--db-path")
            .arg(db_dir.path())
            .args(["--ip", "127.0.0.1", "--port", "0"])
            .args(["--address", "127.0.0.1"])
            .args(extra_args)
            .env("HUMAN_LOGGING", "false")
            .env("RUST_LOG", "info")
            .stdin(Stdio::null())
            .stdout(log.as_file().try_clone()?)
            .stderr(log.as_file().try_clone()?)
            .spawn()
            .with_context(|| {
                format!(
                    "Unable to launch {}. Run version-compatibility/build-historical-node.sh first",
                    executable.display()
                )
            })?;
        let mut process = HistoricalProcess { child, log };
        let result =
            tokio::time::timeout(Duration::from_secs(60), process.client()).await;
        let client = match result {
            Ok(Ok(client)) => client,
            Ok(Err(error)) => {
                return Err(error.context(process.logs()));
            }
            Err(error) => {
                return Err(anyhow::anyhow!(
                    "Historical node startup timed out: {error}\n{}",
                    process.logs()
                ));
            }
        };
        Ok(Self {
            process,
            _db_dir: db_dir,
            client,
        })
    }

    pub async fn kill(mut self) -> tempfile::TempDir {
        #[cfg(unix)]
        {
            use nix::{
                sys::signal::{
                    Signal,
                    kill,
                },
                unistd::Pid,
            };
            kill(
                Pid::from_raw(self.process.child.id() as i32),
                Signal::SIGTERM,
            )
            .expect("Failed to signal historical node shutdown");
            tokio::time::timeout(Duration::from_secs(30), async {
                loop {
                    if let Some(status) = self.process.child.try_wait().unwrap() {
                        assert!(
                            status.success(),
                            "Historical node shutdown failed: {status}\n{}",
                            self.process.logs()
                        );
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            })
            .await
            .expect("Historical node shutdown timed out");
        }
        // On platforms without SIGTERM, Drop still kills and reaps the child.
        drop(self.process);
        self._db_dir
    }
}

define_core_driver!(
    latest_fuel_core_bin,
    LatestFuelService,
    LatestClient,
    LatestFuelCoreDriver,
    true
);

impl LatestFuelCoreDriver {
    pub async fn kill(self) -> tempfile::TempDir {
        self.node
            .send_stop_signal_and_await_shutdown()
            .await
            .expect("Failed to stop the node");
        self._db_dir
    }
}

pub const IGNITION_TESTNET_SNAPSHOT: &str = "./chain-configurations/ignition";
pub const IGNITION_V21_TESTNET_SNAPSHOT: &str = "./chain-configurations/ignition-v21";

pub const V44_TESTNET_SNAPSHOT: &str = "./chain-configurations/v44";
pub const POA_SECRET_KEY: &str =
    "e3d6eb39607650e22f0befa26d52e921d2e7924d0e165f38ffa8d9d0ac73de93";
pub const PRIVILEGED_ADDRESS_KEY: &str =
    "dcbe36d8e890d7489b6e1be442eab98ae2fdbb5c7d77e1f9e1e12a545852304f";
pub const BASE_ASSET_ID: &str =
    "0xf8f8b6283d7fa5b672b530cbb84fcccb4ff8dc40f8176ef4544ddb1f1952ad07";

pub fn default_multiaddr(port: &str, peer_id: PeerId) -> String {
    format!("/ip4/127.0.0.1/tcp/{}/p2p/{}", port, peer_id)
}

pub const SUBSECTION_SIZE: usize = 64 * 1024;

pub fn valid_input(secret_key: &SecretKey, rng: &mut StdRng, amount: u64) -> Input {
    let pk = secret_key.public_key();
    let owner = Input::owner(&pk);
    Input::coin_signed(
        rng.r#gen(),
        owner,
        amount,
        BASE_ASSET_ID.parse().unwrap(),
        Default::default(),
        Default::default(),
    )
}

pub fn transactions_from_subsections(
    rng: &mut StdRng,
    subsections: Vec<UploadSubsection>,
    amount: u64,
) -> Vec<Upload> {
    subsections
        .into_iter()
        .map(|subsection| {
            let secret_key: SecretKey =
                SecretKey::from_str(PRIVILEGED_ADDRESS_KEY).unwrap();
            let mut tx = Transaction::upload_from_subsection(
                subsection,
                Policies::new().with_max_fee(amount),
                vec![valid_input(&secret_key, rng, amount)],
                vec![],
                vec![Witness::default()],
            );
            tx.sign_inputs(&secret_key, &ChainId::new(0));

            tx
        })
        .collect::<Vec<_>>()
}

pub fn upgrade_transaction(
    purpose: UpgradePurpose,
    rng: &mut StdRng,
    amount: u64,
) -> Upgrade {
    let secret_key: SecretKey = SecretKey::from_str(PRIVILEGED_ADDRESS_KEY).unwrap();
    let mut tx = Transaction::upgrade(
        purpose,
        Policies::new().with_max_fee(100000),
        vec![valid_input(&secret_key, rng, amount)],
        vec![],
        vec![Witness::default()],
    );
    tx.sign_inputs(&secret_key, &ChainId::new(0));
    tx
}
