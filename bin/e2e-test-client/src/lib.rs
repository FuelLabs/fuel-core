use std::time::Duration;

pub const CONFIG_FILE_KEY: &str = "FUEL_CORE_E2E_CONFIG";
pub const SYNC_TIMEOUT: Duration = Duration::from_secs(10);

pub mod config;
pub mod test_context;
