use crate::{
    combined_database::CombinedDatabase,
    database::{
        database_description::{off_chain::OffChain, on_chain::OnChain},
        genesis_progress::GenesisMetadata,
        Database,
    },
};
use fuel_core_chain_config::SnapshotReader;
use fuel_core_storage::{
    blueprint::BlueprintInspect,
    iter::IteratorOverTable,
    kv_store::StorageColumn,
    structured_storage::{StructuredStorage, TableWithBlueprint},
    transactional::{InMemoryTransaction, WriteTransaction},
    Mappable, StorageAsMut,
};

use itertools::Itertools;

use super::workers::GenesisWorkers;

/// Performs the importing of the genesis block from the snapshot.
pub async fn execute_genesis_block(
    db: CombinedDatabase,
    snapshot_reader: SnapshotReader,
) -> anyhow::Result<()> {
    // TODO: Should we insert a FuelBlockIdsToHeights entry for the genesis block?
    let mut workers = GenesisWorkers::new(db, snapshot_reader);
    if let Err(e) = workers.run_off_chain_imports().await {
        workers.shutdown();
        workers.finished().await;

        return Err(e);
    }

    Ok(())
}
