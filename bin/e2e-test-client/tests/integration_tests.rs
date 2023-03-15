use assert_cmd::prelude::*;
use fuel_core::service::{
    Config,
    FuelService,
};
// Add methods on commands
use fuel_core_e2e_client::{
    config::SuiteConfig,
    CONFIG_FILE_KEY,
};
use std::{
    fs,
    io::Write,
    process::Command,
};
use tempfile::TempDir; // Used for writing assertions // Run programs

#[tokio::test(flavor = "multi_thread")]
async fn works_in_local_env() -> Result<(), Box<dyn std::error::Error>> {
    // setup a local node
    let srv = setup_local_node().await;
    // generate a config file
    let config = generate_config_file(srv.bound_address.to_string());
    // execute suite
    execute_suite(config.path)
}

// Spins up a node for each wallet and verifies that the suite works across multiple nodes
#[cfg(feature = "p2p")]
#[tokio::test(flavor = "multi_thread")]
async fn works_in_multinode_local_env() -> Result<(), Box<dyn std::error::Error>> {
    use fuel_core::p2p_test_helpers::*;
    use fuel_core_types::{
        fuel_crypto::{
            rand::{
                prelude::StdRng,
                SeedableRng,
            },
            SecretKey,
        },
        fuel_tx::Input,
    };

    let mut rng = StdRng::seed_from_u64(line!() as u64);
    let secret = SecretKey::random(&mut rng);
    let pub_key = Input::owner(&secret.public_key());
    let Nodes {
        mut producers,
        mut validators,
        bootstrap_nodes: _dont_drop,
    } = make_nodes(
        [Some(BootstrapSetup::new(pub_key))],
        [Some(
            ProducerSetup::new(secret).with_txs(1).with_name("Alice"),
        )],
        [Some(ValidatorSetup::new(pub_key).with_name("Bob"))],
    )
    .await;

    let producer = producers.pop().unwrap();
    let validator = validators.pop().unwrap();

    // generate a config file without a default endpoint to
    // verify wallets connect to different nodes
    let mut config = SuiteConfig {
        endpoint: "".to_string(),
        ..Default::default()
    };

    config.wallet_a.endpoint = Some(producer.node.bound_address.to_string());
    config.wallet_b.endpoint = Some(validator.node.bound_address.to_string());

    // save config file
    let config = save_config_file(config);

    // execute suite
    execute_suite(config.path)
}

fn execute_suite(config_path: String) -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("fuel-core-e2e-client")?;
    let cmd = cmd.env(CONFIG_FILE_KEY, config_path).assert().success();
    std::io::stdout()
        .write_all(&cmd.get_output().stdout)
        .unwrap();
    std::io::stderr()
        .write_all(&cmd.get_output().stderr)
        .unwrap();
    Ok(())
}

async fn setup_local_node() -> FuelService {
    FuelService::new_node(Config::local_node()).await.unwrap()
}

fn generate_config_file(endpoint: String) -> TestConfig {
    // setup config for test env
    let config = SuiteConfig {
        endpoint,
        ..Default::default()
    };
    save_config_file(config)
}

fn save_config_file(config: SuiteConfig) -> TestConfig {
    // generate a tmp dir
    let tmp_dir = TempDir::new().unwrap();
    // write config to file
    let config_path = tmp_dir.path().join("config.toml");
    fs::write(&config_path, toml::to_string(&config).unwrap()).unwrap();

    TestConfig {
        path: config_path.to_str().unwrap().to_string(),
        _dir: tmp_dir,
    }
}

struct TestConfig {
    path: String,
    // keep the temp dir alive to defer the deletion of the temp dir until the end of the test
    _dir: TempDir,
}

fuel_core_trace::enable_tracing!();
