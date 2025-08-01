#![allow(dead_code)]
use latest_fuel_core_type::{
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
use std::str::FromStr;
use version_44_fuel_core_bin::FuelService as Version44FuelService;
use version_44_fuel_core_client::client::FuelClient as Version44Client;
use version_44_fuel_core_services as _;

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

define_core_driver!(
    version_44_fuel_core_bin,
    Version44FuelService,
    Version44Client,
    Version44FuelCoreDriver,
    true
);

impl Version44FuelCoreDriver {
    pub async fn kill(self) -> tempfile::TempDir {
        self.node
            .send_stop_signal_and_await_shutdown()
            .await
            .expect("Failed to stop the node");
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
