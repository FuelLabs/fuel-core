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

fn main() -> anyhow::Result<()> {
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
