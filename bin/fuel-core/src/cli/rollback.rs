use crate::cli::default_db_path;
use anyhow::Context;
use clap::Parser;
use fuel_core::{
    combined_database::CombinedDatabase,
    service::genesis::NotifyCancel,
    state::historical_rocksdb::StateRewindPolicy,
};
use std::path::PathBuf;

/// Print a snapshot of blockchain state to stdout.
#[derive(Debug, Clone, Parser)]
pub struct Command {
    /// The path to the database.
    #[clap(
        name = "DB_PATH",
        long = "db-path",
        value_parser,
        default_value = default_db_path().into_os_string()
    )]
    pub database_path: PathBuf,

    /// The path to the database.
    #[clap(long = "target-block-height")]
    pub target_block_height: u32,
}

pub async fn exec(command: Command) -> anyhow::Result<()> {
    use crate::cli::ShutdownListener;

    let path = command.database_path.as_path();
    let db = CombinedDatabase::open(
        path,
        64 * 1024 * 1024,
        StateRewindPolicy::RewindFullRange,
    )
    .map_err(Into::<anyhow::Error>::into)
    .context(format!("failed to open combined database at path {path:?}"))?;

    let shutdown_listener = ShutdownListener::spawn();
    let target_block_height = command.target_block_height.into();

    while !shutdown_listener.is_cancelled() {
        let on_chain_height = db
            .on_chain()
            .latest_height()?
            .ok_or(anyhow::anyhow!("on-chain database doesn't have height"))?;

        let off_chain_height = db
            .off_chain()
            .latest_height()?
            .ok_or(anyhow::anyhow!("on-chain database doesn't have height"))?;

        if on_chain_height == target_block_height
            && off_chain_height == target_block_height
        {
            break;
        }

        if off_chain_height == target_block_height
            && on_chain_height < target_block_height
        {
            return Err(anyhow::anyhow!(
                "on-chain database height is less than target height"
            ));
        }

        if on_chain_height == target_block_height
            && off_chain_height < target_block_height
        {
            return Err(anyhow::anyhow!(
                "off-chain database height is less than target height"
            ));
        }

        if on_chain_height > target_block_height {
            db.on_chain().rollback_last_block()?;
            tracing::info!(
                "Rolled back on-chain database to height {:?}",
                on_chain_height.pred()
            );
        }

        if off_chain_height > target_block_height {
            db.off_chain().rollback_last_block()?;
            tracing::info!(
                "Rolled back off-chain database to height {:?}",
                on_chain_height.pred()
            );
        }
    }
    Ok(())
}
