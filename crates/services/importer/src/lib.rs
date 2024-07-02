#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

use fuel_core_types::services::block_importer::SharedImportResult;

pub mod config;
pub mod importer;
pub mod ports;

pub use config::Config;
pub use importer::Importer;

#[derive(Clone)]
pub struct ImporterResult {
    pub shared_result: SharedImportResult,
    #[cfg(feature = "test-helpers")]
    pub changes: std::sync::Arc<fuel_core_storage::transactional::Changes>,
}

impl core::ops::Deref for ImporterResult {
    type Target = SharedImportResult;

    fn deref(&self) -> &Self::Target {
        &self.shared_result
    }
}

#[cfg(test)]
fuel_core_trace::enable_tracing!();
