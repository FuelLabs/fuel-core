// Use Jemalloc for main binary

#[cfg(feature = "jemalloc")]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use fuel_core_bin::cli;
use std::time::Duration;

/// Upper bound for joining the main runtime's worker threads on shutdown.
/// `#[tokio::main]`'s default Drop uses `shutdown_background`, which detaches
/// workers and lets them outlive `main` — they then race rocksdb's C++ static
/// destructors during libc `atexit` and crash the process. Joining the workers
/// synchronously before `main` returns avoids that race.
const RUNTIME_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// Disable coredumps for this process unless explicitly enabled.
///
/// Rocksdb's C++ static destructors race fuel-core's own DB teardown at libc
/// `atexit`, raising `SIGABRT` ("pthread lock: Invalid argument"). The
/// application-level shutdown is already clean by that point — the WAL is
/// synced and rocksdb recovers on next start — but the `SIGABRT` triggers a
/// kernel coredump of the entire RSS (~2.5 GB), which dominates pod-restart
/// time on Kubernetes (~60 s vs ~6 s) without producing actionable
/// diagnostics. The race is upstream of us (see `rust-lang/rust#83994`,
/// `rust-rocksdb#270`, `facebook/rocksdb#3453`); skipping the coredump is
/// the canonical workaround.
///
/// To keep coredumps on (e.g. when debugging a real crash) set
/// `FUEL_CORE_ENABLE_COREDUMP` to a truthy value (`1`, `true`, `yes`, `on`,
/// case-insensitive). Any other value — including `0`, `false`, or an empty
/// string — leaves coredumps disabled, so operators can explicitly opt out
/// in their manifests without accidentally re-enabling them.
#[cfg(feature = "rocksdb")]
fn disable_coredump_unless_opted_in() {
    let opted_in = std::env::var("FUEL_CORE_ENABLE_COREDUMP")
        .ok()
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false);
    if opted_in {
        return;
    }
    if let Err(e) = rlimit::setrlimit(rlimit::Resource::CORE, 0, 0) {
        eprintln!("Warning: failed to disable coredumps: {e}");
    }
}

fn main() -> anyhow::Result<()> {
    #[cfg(feature = "rocksdb")]
    disable_coredump_unless_opted_in();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let result = runtime.block_on(cli::run_cli());

    // Synchronously join the runtime's workers before returning to `main`,
    // so they cannot drop `Arc<rocksdb::PrimaryInstance>` clones from a
    // worker thread after libc has started destroying rocksdb's static
    // singletons.
    runtime.shutdown_timeout(RUNTIME_SHUTDOWN_TIMEOUT);

    result
}
