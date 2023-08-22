use rand::Rng;
use serial_test::serial;
use std::{
    env,
    path::PathBuf,
    time::Duration,
};
use tokio::{
    io::AsyncReadExt,
    process::Command,
    time::sleep,
};

const BIN_NAME: &str = "/Volumes/Fuel/projects/FuelLabs/fuel-core/target/debug/fuel-core";

fn generate_db_path() -> PathBuf {
    let mut rng = rand::thread_rng();
    let random: String = (0..8)
        .map(|_| rng.gen_range(0..=15))
        .map(|n| format!("{:x}", n))
        .collect();
    let random_dir = PathBuf::from(random);

    let mut temp_path = env::temp_dir();
    temp_path.push(random_dir);

    temp_path
}

#[tokio::test]
#[serial]
async fn test_run_with_valid_args_produces_no_error() {
    let db_path = generate_db_path();
    let db_path_arg = format!("--db-path={db_path}", db_path = db_path.display());

    let port = 4444;
    let port_arg = format!("--port={port}");

    let args = [
        "run",
        "--enable-p2p",
        "--enable-relayer",
        &db_path_arg,
        &port_arg,
    ];
    let mut command = Command::new(BIN_NAME);
    let bin = command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .args(args);

    let mut child = bin.spawn().expect("Expected binary to execute");
    let stderr = child.stderr.take().expect("Expected stderr to be present");
    let mut reader = tokio::io::BufReader::new(stderr);
    let mut output = String::new();

    let timeout_duration = Duration::from_secs(3);
    let mut error = None;

    loop {
        tokio::select! {
            _ = sleep(timeout_duration) => {
                child.kill().await.expect("Expected kill");
                break;
            },
            _ = reader.read_to_string(&mut output) => {
                let e = output.clone();
                error = Some(e);
                break;
            }
        }
    }

    child.wait().await.expect("expected status");

    assert_eq!(error, None);
}

#[tokio::test]
#[serial]
async fn test_run_with_invalid_args_produces_invalid_arg_error() {
    let args = ["run", "--invalid-argument"];
    let mut command = Command::new(BIN_NAME);
    let bin = command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .args(args);

    let mut child = bin.spawn().expect("Expected binary to execute");
    let stderr = child.stderr.take().expect("Expected stderr to be present");
    let mut reader = tokio::io::BufReader::new(stderr);
    let mut output = String::new();

    let timeout_duration = Duration::from_secs(3);
    let mut error = None;

    loop {
        tokio::select! {
            _ = sleep(timeout_duration) => {
                child.kill().await.expect("Expected kill");
                break;
            },
            _ = reader.read_to_string(&mut output) => {
                let e = output.clone();
                error = Some(e);
                break;
            }
        }
    }

    child.wait().await.expect("expected status");

    assert!(error.is_some());
}
