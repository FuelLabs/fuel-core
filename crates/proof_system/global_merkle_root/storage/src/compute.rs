use fuel_core_storage::{
    blueprint::BlueprintInspect,
    kv_store::{
        KeyValueInspect,
        StorageColumn,
    },
    structured_storage::TableWithBlueprint,
    tables::{
        BlobData,
        Coins,
        ConsensusParametersVersions,
        ContractsLatestUtxo,
        ContractsRawCode,
        Messages,
        ProcessedTransactions,
        StateTransitionBytecodeVersions,
        UploadedBytecodes,
    },
    transactional::StorageTransaction,
    Error as StorageError,
    Mappable,
    MerkleRoot,
    MerkleRootStorage,
};
use fuel_core_types::{
    fuel_crypto::Hasher,
    fuel_tx::Bytes32,
};

use crate::{
    column::Column,
    merkle::{
        DummyStorage,
        Merkleized,
        MerkleizedTableColumn,
    },
};

/// Main entrypoint for the state root computation
pub trait ComputeStateRoot {
    /// Compute the global state root
    fn state_root(&self) -> Result<Bytes32, StorageError>;

    /// Compute the merkle root of the specific table
    fn merkle_root<Table>(&self) -> Result<MerkleRoot, StorageError>
    where
        Table: Mappable + MerkleizedTableColumn,
        Table: TableWithBlueprint,
        Table::Blueprint: BlueprintInspect<Table, DummyStorage<Column>>;
}

impl<Storage> ComputeStateRoot for StorageTransaction<Storage>
where
    Storage: KeyValueInspect<Column = Column>,
{
    fn state_root(&self) -> Result<Bytes32, StorageError> {
        let roots = [
            self.merkle_root::<ContractsRawCode>()?,
            self.merkle_root::<ContractsLatestUtxo>()?,
            self.merkle_root::<Coins>()?,
            self.merkle_root::<Messages>()?,
            self.merkle_root::<ProcessedTransactions>()?,
            self.merkle_root::<ConsensusParametersVersions>()?,
            self.merkle_root::<StateTransitionBytecodeVersions>()?,
            self.merkle_root::<UploadedBytecodes>()?,
            self.merkle_root::<BlobData>()?,
        ];

        let mut hasher = Hasher::default();

        for root in roots {
            hasher.input(root);
        }

        Ok(hasher.finalize())
    }

    fn merkle_root<Table>(&self) -> Result<MerkleRoot, StorageError>
    where
        Table: Mappable + MerkleizedTableColumn,
        Table: TableWithBlueprint,
        Table::Blueprint: BlueprintInspect<Table, DummyStorage<Column>>,
    {
        <Self as MerkleRootStorage<u32, Merkleized<Table>>>::root(
            self,
            &Table::column().id(),
        )
    }
}
