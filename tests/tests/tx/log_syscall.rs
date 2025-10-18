#![allow(non_snake_case)]

use crate::helpers::TestSetupBuilder;
use fuel_core::chain_config::{
    self,
    CoinConfig,
    SnapshotWriter,
};
use fuel_core_trace::subscriber::capture_logs_async;
use fuel_core_types::{
    fuel_asm::*,
    fuel_tx::*,
};
use rand::{
    Rng,
    SeedableRng,
    rngs::StdRng,
};
use test_helpers::fuel_core_driver::FuelCoreDriver;

const SYSCALL_LOG: u64 = 1000;
const LOG_FD_STDOUT: u64 = 1;

fn setup_tx(
    base_asset_id: AssetId,
    predicate_msgs: &[&str],
    script_msg: Option<&str>,
) -> (Transaction, Vec<CoinConfig>) {
    let mut rng = StdRng::seed_from_u64(121210);

    const AMOUNT: u64 = 1_000;

    let text_reg = 0x10;
    let text_size_reg = 0x11;
    let syscall_id_reg = 0x12;
    let fd_reg = 0x13;
    let tmp_reg = 0x14;

    let predicates: Vec<_> = predicate_msgs
        .iter()
        .map(|msg| {
            let predicate_data: Vec<u8> = msg.bytes().collect();
            let predicate = vec![
                op::movi(syscall_id_reg, SYSCALL_LOG as u32),
                op::movi(fd_reg, LOG_FD_STDOUT as u32),
                op::gm_args(tmp_reg, GMArgs::GetVerifyingPredicate),
                op::gtf_args(text_reg, tmp_reg, GTFArgs::InputCoinPredicateData),
                op::movi(text_size_reg, predicate_data.len().try_into().unwrap()),
                op::ecal(syscall_id_reg, fd_reg, text_reg, text_size_reg),
                op::ret(RegId::ONE),
            ]
            .into_iter()
            .collect();
            let predicate_owner = Input::predicate_owner(&predicate);
            (predicate, predicate_data, predicate_owner)
        })
        .collect();

    let (script, script_data) = match script_msg {
        Some(msg) => {
            let script_data: Vec<u8> = msg.bytes().collect();
            let script: Vec<u8> = vec![
                op::movi(syscall_id_reg, SYSCALL_LOG as u32),
                op::movi(fd_reg, LOG_FD_STDOUT as u32),
                op::gtf_args(text_reg, 0x00, GTFArgs::ScriptData),
                op::movi(text_size_reg, script_data.len().try_into().unwrap()),
                op::ecal(syscall_id_reg, fd_reg, text_reg, text_size_reg),
                op::ret(RegId::ONE),
            ]
            .into_iter()
            .collect();
            (script, script_data)
        }
        None => (Default::default(), Default::default()),
    };

    // Given
    let mut tx = TransactionBuilder::script(script, script_data);
    tx.script_gas_limit(1_000_000);
    for (predicate, predicate_data, predicate_owner) in predicates {
        tx.add_input(Input::coin_predicate(
            rng.r#gen(),
            predicate_owner,
            AMOUNT,
            base_asset_id,
            Default::default(),
            Default::default(),
            predicate,
            predicate_data,
        ));
    }
    let tx = tx.finalize();
    let mut context = TestSetupBuilder::default();
    context.config_coin_inputs_from_transactions(&[&tx]);
    (tx.into(), context.initial_coins)
}

struct TestCtx {
    /// This must be before the db_dir as the drop order matters here
    driver: FuelCoreDriver,
    _db_dir: tempfile::TempDir,
    tx: Transaction,
    tx_id: TxId,
}

impl TestCtx {
    async fn setup(
        allow_syscall: bool,
        predicate_msgs: &[&str],
        script_msg: Option<&str>,
    ) -> anyhow::Result<TestCtx> {
        let cc = chain_config::ChainConfig::local_testnet();

        let base_asset_id = cc.consensus_parameters.base_asset_id();
        let (tx, initial_coins) = setup_tx(*base_asset_id, predicate_msgs, script_msg);

        let temp_dir = tempfile::tempdir()?;

        let snapshot_dir = temp_dir.path().join("snapshot");
        let db_path = temp_dir.path().join("db");
        std::fs::create_dir(&db_path)?;

        let state_config = chain_config::StateConfig {
            coins: initial_coins,
            messages: Vec::new(),
            blobs: Vec::new(),
            contracts: Vec::new(),
            last_block: None,
        };

        let snapshot_writer = SnapshotWriter::json(snapshot_dir.clone());
        snapshot_writer.write_state_config(state_config, &cc)?;

        let mut args = vec![
            "--db-type",
            "in-memory",
            "--debug",
            "--utxo-validation",
            "--snapshot",
            snapshot_dir.as_path().to_str().unwrap(),
        ];

        if allow_syscall {
            args.push("--allow-syscall");
        }

        let driver = FuelCoreDriver::spawn(&args).await?;

        let chain_id = driver
            .client
            .chain_info()
            .await
            .unwrap()
            .consensus_parameters
            .chain_id();
        let tx_id = tx.id(&chain_id);

        Ok(TestCtx {
            driver,
            _db_dir: temp_dir,
            tx,
            tx_id,
        })
    }
}

fn extract_logs_so_far(tx_id: TxId, lines: Vec<String>) -> Logs {
    let anchor_verification_logs = format!("verification{{tx_id={tx_id}}}");
    let anchor_execution_logs = format!("execution{{tx_id={tx_id}}}");
    let anchor_estimation_logs = format!("estimation{{tx_id={tx_id}}}");

    // Log lines start with ISO 8601 timestamp, e.g. "2024-06-14T12:34:56.789Z".
    // We use this to detect the start of a log lines.
    let re_timestamp =
        regex::Regex::new(r"^\d\d\d\d-\d\d-\d\dT\d\d:\d\d:\d\d\.\d+Z").unwrap();

    let mut result = Logs::default();

    let mut it = lines.into_iter().peekable();
    loop {
        let Some(line) = it.next() else {
            break;
        };
        if line.contains(&anchor_verification_logs) {
            while it
                .peek()
                .map(|line| !re_timestamp.is_match(line))
                .unwrap_or(false)
            {
                result.verification.push(it.next().unwrap());
            }
        }
        if line.contains(&anchor_execution_logs) {
            assert!(
                result.execution.is_empty(),
                "Multiple execution logs sections found in output"
            );
            while it
                .peek()
                .map(|line| !re_timestamp.is_match(line))
                .unwrap_or(false)
            {
                result.execution.push(it.next().unwrap());
            }
        }
        if line.contains(&anchor_estimation_logs) {
            while it
                .peek()
                .map(|line| !re_timestamp.is_match(line))
                .unwrap_or(false)
            {
                result.estimation.push(it.next().unwrap());
            }
        }
    }

    result
}

#[derive(Default)]
struct Logs {
    verification: Vec<String>,
    execution: Vec<String>,
    estimation: Vec<String>,
}

const PREDICATE_0_LOG: &str = "Hello from Predicate 0!";
const PREDICATE_1_LOG: &str = "Hello from Predicate 1!";
const SCRIPT_LOG: &str = "Hello from Script!";

#[test]
fn estimate_predicates__print_predicate_logs() {
    let (tx_id, logs) = capture_logs_async(async {
        // Given
        let mut ctx =
            TestCtx::setup(true, &[PREDICATE_0_LOG, PREDICATE_1_LOG], Some(SCRIPT_LOG))
                .await
                .expect("Failed to setup test context");

        // When
        ctx.driver
            .client
            .estimate_predicates(&mut ctx.tx)
            .await
            .unwrap();

        ctx.tx_id
    });

    // Then
    let logs = extract_logs_so_far(tx_id, logs);
    assert!(
        logs.verification.is_empty(),
        "Found verification logs in output"
    );
    assert!(logs.execution.is_empty(), "Found execution logs in output");
    assert!(logs.estimation[0].contains("[predicate 0"));
    assert!(logs.estimation[0].contains(PREDICATE_0_LOG));
    assert!(logs.estimation[1].contains("[predicate 1"));
    assert!(logs.estimation[1].contains(PREDICATE_1_LOG));
}

#[test]
fn dry_run__produces_syscall_logs_for_both_script_and_predicates() -> anyhow::Result<()> {
    let (tx_id, logs) = capture_logs_async(async {
        // Given
        let mut ctx =
            TestCtx::setup(true, &[PREDICATE_0_LOG, PREDICATE_1_LOG], Some(SCRIPT_LOG))
                .await
                .expect("Failed to setup test context");
        ctx.driver
            .client
            .estimate_predicates(&mut ctx.tx)
            .await
            .expect("Failed to estimate predicates");

        // When
        let result = ctx.driver.client.dry_run(&[ctx.tx.clone()]).await;

        // Then
        result.expect("Transaction should be executed successfully");
        ctx.tx_id
    });

    let logs = extract_logs_so_far(tx_id, logs);

    assert_eq!(logs.verification.len(), 2, "Expected 2 predicate logs");
    assert!(logs.verification[0].contains("[predicate 0"));
    assert!(logs.verification[0].contains(PREDICATE_0_LOG));
    assert!(logs.verification[1].contains("[predicate 1"));
    assert!(logs.verification[1].contains(PREDICATE_1_LOG));

    assert_eq!(logs.execution.len(), 1, "Expected 1 tx log line");
    assert!(logs.execution[0].contains("[script,"));
    assert!(logs.execution[0].contains(SCRIPT_LOG));

    assert_eq!(logs.estimation.len(), 2, "Expected 2 estimation logs");

    Ok(())
}

#[test]
fn submit_and_await_commit__produces_syscall_logs_for_all() -> anyhow::Result<()> {
    let (tx_id, logs) = capture_logs_async(async {
        // Given
        let mut ctx =
            TestCtx::setup(true, &[PREDICATE_0_LOG, PREDICATE_1_LOG], Some(SCRIPT_LOG))
                .await
                .expect("Failed to setup test context");
        ctx.driver
            .client
            .estimate_predicates(&mut ctx.tx)
            .await
            .expect("Failed to estimate predicates");

        // When
        let result = ctx
            .driver
            .client
            .submit_and_await_commit(&ctx.tx.clone())
            .await;

        // Then
        result.expect("Transaction should be executed successfully");
        ctx.tx_id
    });

    let logs = extract_logs_so_far(tx_id, logs);

    assert_eq!(logs.verification.len(), 2, "Expected 2 predicate logs");
    assert!(logs.verification[0].contains("[predicate 0"));
    assert!(logs.verification[0].contains(PREDICATE_0_LOG));
    assert!(logs.verification[1].contains("[predicate 1"));
    assert!(logs.verification[1].contains(PREDICATE_1_LOG));

    assert_eq!(logs.execution.len(), 1, "Expected 1 tx log line");
    assert!(logs.execution[0].contains("[script,"));
    assert!(logs.execution[0].contains(SCRIPT_LOG));

    assert_eq!(logs.estimation.len(), 2, "Expected 2 predicate logs");
    assert!(logs.estimation[0].contains("[predicate 0"));
    assert!(logs.estimation[0].contains(PREDICATE_0_LOG));
    assert!(logs.estimation[1].contains("[predicate 1"));
    assert!(logs.estimation[1].contains(PREDICATE_1_LOG));

    Ok(())
}

#[test]
fn submit_and_await_commit__syscall_logs_allow_special_characters() -> anyhow::Result<()>
{
    let (tx_id, logs) = capture_logs_async(async {
        // Given
        let mut ctx = TestCtx::setup(true, &["Special\nCharacters:€π≈!"], None)
            .await
            .expect("Failed to setup test context");
        ctx.driver
            .client
            .estimate_predicates(&mut ctx.tx)
            .await
            .expect("Failed to estimate predicates");

        // When
        let result = ctx
            .driver
            .client
            .submit_and_await_commit(&ctx.tx.clone())
            .await;

        // Then
        result.expect("Transaction should be executed successfully");
        ctx.tx_id
    });

    let logs = extract_logs_so_far(tx_id, logs);

    assert_eq!(logs.verification.len(), 2, "Expected 2 predicate log lines");
    assert!(logs.verification[0].contains("[predicate 0"));
    assert!(logs.verification[0].contains("] stdout: Special"));
    assert!(logs.verification[1].contains("Characters:€π≈!"));

    assert!(logs.execution.is_empty(), "Expected no execution logs");

    Ok(())
}

#[test]
fn dry_run__syscall_logs_are_not_printed_when_none_are_present() -> anyhow::Result<()> {
    let (tx_id, logs) = capture_logs_async(async {
        // Given
        let mut ctx = TestCtx::setup(false, &[], None)
            .await
            .expect("Failed to setup test context");
        ctx.driver
            .client
            .estimate_predicates(&mut ctx.tx)
            .await
            .expect("Failed to estimate predicates");

        // When
        let result = ctx.driver.client.dry_run(&[ctx.tx.clone()]).await;

        // Then
        result.expect_err("Transaction execution should fail");

        ctx.tx_id
    });

    let logs = extract_logs_so_far(tx_id, logs);
    assert!(
        logs.verification.is_empty(),
        "Expected no verification logs"
    );
    assert!(logs.execution.is_empty(), "Expected no execution logs");
    assert!(logs.estimation.is_empty(), "Expected no estimation logs");

    Ok(())
}

#[test]
fn submit_and_await_commit__syscall_logs_are_not_printed_when_none_are_present()
-> anyhow::Result<()> {
    let (tx_id, logs) = capture_logs_async(async {
        // Given
        let mut ctx = TestCtx::setup(false, &[], None)
            .await
            .expect("Failed to setup test context");
        ctx.driver
            .client
            .estimate_predicates(&mut ctx.tx)
            .await
            .expect("Failed to estimate predicates");

        // When
        let result = ctx
            .driver
            .client
            .submit_and_await_commit(&ctx.tx.clone())
            .await;

        // Then
        result.expect_err("Transaction execution should fail");
        ctx.tx_id
    });

    let logs = extract_logs_so_far(tx_id, logs);
    assert!(
        logs.verification.is_empty(),
        "Expected no verification logs"
    );
    assert!(logs.execution.is_empty(), "Expected no execution logs");
    assert!(logs.estimation.is_empty(), "Expected no estimation logs");

    Ok(())
}

#[test]
fn dry_run__syscall_logs_cause_error_when_not_enabled() -> anyhow::Result<()> {
    let (tx_id, logs) = capture_logs_async(async {
        // Given
        let mut ctx = TestCtx::setup(false, &["Special\nCharacters:€π≈!"], None)
            .await
            .expect("Failed to setup test context");
        ctx.driver
            .client
            .estimate_predicates(&mut ctx.tx)
            .await
            .expect("Failed to estimate predicates");

        // When
        let result = ctx.driver.client.dry_run(&[ctx.tx.clone()]).await;

        // Then
        result.expect_err("Transaction execution should fail");

        ctx.tx_id
    });

    let logs = extract_logs_so_far(tx_id, logs);
    assert!(
        logs.verification.is_empty(),
        "Expected no verification logs"
    );
    assert!(logs.execution.is_empty(), "Expected no execution logs");
    assert!(logs.estimation.is_empty(), "Expected no estimation logs");

    Ok(())
}

#[test]
fn submit_and_await_commit__syscall_logs_cause_error_when_not_enabled()
-> anyhow::Result<()> {
    let (tx_id, logs) = capture_logs_async(async {
        // Given
        let mut ctx = TestCtx::setup(false, &["Special\nCharacters:€π≈!"], None)
            .await
            .expect("Failed to setup test context");
        ctx.driver
            .client
            .estimate_predicates(&mut ctx.tx)
            .await
            .expect("Failed to estimate predicates");

        // When
        let result = ctx
            .driver
            .client
            .submit_and_await_commit(&ctx.tx.clone())
            .await;

        // Then
        result.expect_err("Transaction execution should fail");

        ctx.tx_id
    });

    let logs = extract_logs_so_far(tx_id, logs);
    assert!(
        logs.verification.is_empty(),
        "Expected no verification logs"
    );
    assert!(logs.execution.is_empty(), "Expected no execution logs");
    assert!(logs.estimation.is_empty(), "Expected no estimation logs");

    Ok(())
}
