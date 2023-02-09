//! The `e2e-test-client` binary is used to run end-to-end tests for the Fuel client.

use libtest_mimic::{
    Arguments,
    Failed,
    Trial,
};
use std::{
    env,
    thread,
    time,
};

pub mod transfer;

fn main() {
    let args = Arguments::from_args();

    let tests = vec![
        Trial::test("check_toph", check_toph),
        Trial::test("long_computation", long_computation).with_ignored_flag(true),
        Trial::test("foo", compile_fail_dummy).with_kind("compile-fail"),
        Trial::test("check_katara", check_katara),
    ];

    libtest_mimic::run(&args, tests).exit();
}

// Tests

fn check_toph() -> Result<(), Failed> {
    Ok(())
}
fn check_katara() -> Result<(), Failed> {
    Ok(())
}
fn long_computation() -> Result<(), Failed> {
    thread::sleep(time::Duration::from_secs(1));
    Ok(())
}
fn compile_fail_dummy() -> Result<(), Failed> {
    Ok(())
}
