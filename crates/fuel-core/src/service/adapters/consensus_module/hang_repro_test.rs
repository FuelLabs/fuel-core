//! Regression tests for the 2026-04-22 mainnet hang.
//!
//! On v0.47.4 the deployed binary linked `redis 0.27.6`. That release of
//! `redis::Client::get_connection_with_timeout` only applied the timeout to
//! the TCP connect step; the post-connect handshake pipeline (`CLIENT
//! SETINFO LIB-NAME`, `CLIENT SETINFO LIB-VER`) ran with no socket-level
//! read/write timeout. A peer that accepted TCP but did not respond to the
//! handshake hung the call indefinitely. Combined with the previous
//! `std::thread::scope`-based `publish_block_on_all_nodes`, one stuck thread
//! wedged every block publish forever and halted block production.
//!
//! The fix in this PR has two parts:
//!
//!   1. Bump `redis` to 1.2, which sets read/write timeouts on the socket
//!      *before* running the handshake pipeline (see
//!      `redis::connection::connect`). A half-alive peer now fails the
//!      `get_connection_with_timeout` call within `node_timeout` instead of
//!      hanging.
//!
//!   2. Replace `std::thread::scope` in `publish_block_on_all_nodes` with
//!      detached `std::thread::spawn` workers that report into an mpsc
//!      channel. The publish returns as soon as `Written` quorum is
//!      reached. Stragglers — including any future single-thread hang that
//!      survives per-syscall timeouts — are abandoned and don't block the
//!      caller.
//!
//! These tests assert (1) directly. The adapter-level coverage for (2)
//! lives in the existing test suite — adding a multi-node mock harness
//! here is intentionally out of scope for the hotfix; this file is the
//! regression guard for the upstream library bug specifically.
//!
//! Run with:
//!   cargo test -p fuel-core --lib \
//!     service::adapters::consensus_module::hang_repro_test -- --nocapture

#![cfg(test)]

use std::{
    io::Read,
    net::{
        SocketAddrV4,
        TcpListener,
    },
    sync::mpsc,
    thread,
    time::{
        Duration,
        Instant,
    },
};

/// Spawn a TCP listener that accepts connections and drains incoming bytes
/// into a buffer but never writes anything back. Emulates an ElastiCache /
/// Valkey node that is "half-alive": TCP accepts, commands hang.
///
/// Returns the bound port. The listener thread is detached and runs for the
/// lifetime of the test process.
fn spawn_halflive_redis_listener() -> u16 {
    let listener = TcpListener::bind(SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, 0))
        .expect("bind ephemeral port");
    let port = listener.local_addr().unwrap().port();

    thread::spawn(move || {
        for incoming in listener.incoming() {
            let Ok(mut stream) = incoming else {
                continue;
            };
            thread::spawn(move || {
                let mut buf = [0u8; 4096];
                while let Ok(n) = stream.read(&mut buf) {
                    if n == 0 {
                        return;
                    }
                }
            });
        }
    });

    port
}

/// Asserts that `redis::Client::get_connection_with_timeout(node_timeout)`
/// returns within a bounded time when the peer is half-alive.
///
/// On `redis 0.27.x` this call hangs indefinitely; the test would never
/// return a result and would deadlock or be killed by the test timeout.
/// On `redis >= 1.2` it returns an `Err` within roughly `2 * node_timeout`
/// (one timeout per pipeline command in the handshake).
///
/// This is the regression guard for the redis-crate upgrade.
#[test]
fn redis_get_connection_with_timeout_is_bounded_against_halflive_peer() {
    const NODE_TIMEOUT: Duration = Duration::from_secs(1);
    // Allow generous wall clock above the per-syscall timeout. We bound the
    // assertion at 5 * node_timeout so a slow CI machine still passes; the
    // pre-fix behavior would never return.
    const MAX_WAIT: Duration = Duration::from_secs(5);

    let port = spawn_halflive_redis_listener();
    let url = format!("redis://127.0.0.1:{port}/");
    let client = redis::Client::open(url).expect("client open");

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let start = Instant::now();
        let result = client.get_connection_with_timeout(NODE_TIMEOUT);
        let _ = tx.send((start.elapsed(), result.is_ok()));
    });

    match rx.recv_timeout(MAX_WAIT) {
        Ok((elapsed, ok)) => {
            assert!(
                !ok,
                "expected an error against a half-alive peer, got Ok \
                 after {elapsed:?}",
            );
            assert!(
                elapsed <= MAX_WAIT,
                "call took {elapsed:?}, expected to be bounded under \
                 {MAX_WAIT:?}",
            );
            eprintln!(
                "OK: get_connection_with_timeout returned in {elapsed:?} \
                 with err (bug fixed in this redis-crate version)"
            );
        }
        Err(_) => {
            panic!(
                "REGRESSION: get_connection_with_timeout did not return \
                 within {MAX_WAIT:?} against a half-alive peer. The \
                 redis-crate upstream bug from 0.27.x has reappeared. \
                 Investigate the redis dependency version and \
                 `connect()` timeout handling.",
            );
        }
    }
}
