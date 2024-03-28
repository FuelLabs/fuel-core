use fuel_core_types::services::block_importer::UncommittedResult as UncommittedImportResult;

pub mod on_chain;
mod runner;
mod workers;

pub use runner::GenesisRunner;

use crate::{
    combined_database::CombinedDatabase,
    database::{
        database_description::{off_chain::OffChain, on_chain::OnChain},
        genesis_progress::GenesisMetadata,
        Database,
    },
    service::{config::Config, genesis::UncommittedImportResult},
};
use anyhow::anyhow;
use fuel_core_chain_config::{GenesisCommitment, TableEntry};
use fuel_core_storage::{
    iter::IteratorOverTable,
    tables::{
        Coins, ConsensusParametersVersions, ContractsLatestUtxo, ContractsRawCode,
        Messages, StateTransitionBytecodeVersions,
    },
    transactional::{Changes, StorageTransaction, WriteTransaction},
    StorageAsMut,
};
use fuel_core_types::{
    self,
    blockchain::{
        block::Block,
        consensus::{Consensus, Genesis},
        header::{
            ApplicationHeader, ConsensusHeader, ConsensusParametersVersion,
            PartialBlockHeader, StateTransitionBytecodeVersion,
        },
        primitives::{DaBlockHeight, Empty},
        SealedBlock,
    },
    entities::{coins::coin::Coin, Message},
    fuel_types::{BlockHeight, Bytes32},
    services::block_importer::ImportResult,
};

use itertools::Itertools;

use self::workers::GenesisWorkers;

mod off_chain {
    use crate::combined_database::CombinedDatabase;
    use fuel_core_chain_config::SnapshotReader;

    use super::workers::GenesisWorkers;

    pub async fn import_state(
        db: CombinedDatabase,
        snapshot_reader: SnapshotReader,
    ) -> anyhow::Result<()> {
        let mut workers = GenesisWorkers::new(db, snapshot_reader);
        // TODO: Should we insert a FuelBlockIdsToHeights entry for the genesis block?
        if let Err(e) = workers.run_off_chain_imports().await {
            workers.shutdown();
            workers.finished().await;

            return Err(e);
        }
        Ok(())
    }
}

/// Performs the importing of the genesis block from the snapshot.
pub async fn execute_genesis_block(
    config: &Config,
    db: &CombinedDatabase,
) -> anyhow::Result<UncommittedImportResult<Changes>> {
    on_chain::import_state(db.clone(), config.snapshot_reader.clone()).await?;
    off_chain::import_state(db.clone(), config.snapshot_reader.clone()).await?;

    let genesis_progress_on_chain: Vec<String> = db
        .on_chain()
        .iter_all::<GenesisMetadata<OnChain>>(None)
        .map_ok(|(k, _)| k)
        .try_collect()?;
    let genesis_progress_off_chain: Vec<String> = db
        .off_chain()
        .iter_all::<GenesisMetadata<OffChain>>(None)
        .map_ok(|(k, _)| k)
        .try_collect()?;

    let chain_config = config.snapshot_reader.chain_config();
    let genesis = Genesis {
        // TODO: We can get the serialized consensus parameters from the database.
        //  https://github.com/FuelLabs/fuel-core/issues/1570
        chain_config_hash: chain_config.root()?.into(),
        coins_root: db.on_chain().genesis_coins_root()?.into(),
        messages_root: db.on_chain().genesis_messages_root()?.into(),
        contracts_root: db.on_chain().genesis_contracts_root()?.into(),
    };

    let block = create_genesis_block(config);
    let consensus = Consensus::Genesis(genesis);
    let block = SealedBlock {
        entity: block,
        consensus,
    };

    let mut on_chain = db.on_chain().clone();
    let mut database_transaction_on_chain = on_chain.write_transaction();
    let mut off_chain = db.off_chain().clone();
    let mut database_transaction_off_chain = off_chain.write_transaction();

    // TODO: The chain config should be part of the snapshot state.
    //  https://github.com/FuelLabs/fuel-core/issues/1570
    database_transaction_on_chain
        .storage_as_mut::<ConsensusParametersVersions>()
        .insert(
            &ConsensusParametersVersion::MIN,
            &chain_config.consensus_parameters,
        )?;
    // TODO: The bytecode of the state transition function should be part of the snapshot state.
    //  https://github.com/FuelLabs/fuel-core/issues/1570
    database_transaction_on_chain
        .storage_as_mut::<StateTransitionBytecodeVersions>()
        .insert(&ConsensusParametersVersion::MIN, &[])?;

    // Needs to be given the progress because `iter_all` is not implemented on db transactions.
    for key in genesis_progress_on_chain {
        database_transaction_on_chain
            .storage_as_mut::<GenesisMetadata<OnChain>>()
            .remove(&key)?;
    }
    for key in genesis_progress_off_chain {
        database_transaction_off_chain
            .storage_as_mut::<GenesisMetadata<OffChain>>()
            .remove(&key)?;
    }

    // TODO: We must do atomic commit of both databases somehow

    let result = UncommittedImportResult::new(
        ImportResult::new_from_local(block, vec![], vec![]),
        database_transaction_on_chain.into_changes(),
    );

    Ok(result)
}

pub fn create_genesis_block(config: &Config) -> Block {
    let block_height = config.snapshot_reader.block_height();
    let da_block_height = config.snapshot_reader.da_block_height();
    let transactions = vec![];
    let message_ids = &[];
    let events = Default::default();
    Block::new(
        PartialBlockHeader {
            application: ApplicationHeader::<Empty> {
                da_height: da_block_height,
                // After regenesis, we don't need to support old consensus parameters,
                // so we can start the versioning from the beginning.
                consensus_parameters_version: ConsensusParametersVersion::MIN,
                // After regenesis, we don't need to support old state transition functions,
                // so we can start the versioning from the beginning.
                state_transition_bytecode_version: StateTransitionBytecodeVersion::MIN,
                generated: Empty,
            },
            consensus: ConsensusHeader::<Empty> {
                prev_root: Bytes32::zeroed(),
                height: block_height,
                time: fuel_core_types::tai64::Tai64::UNIX_EPOCH,
                generated: Empty,
            },
        },
        transactions,
        message_ids,
        events,
    )
}
