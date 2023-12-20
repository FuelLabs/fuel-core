use crate::database::{
    Column,
    Database,
};
use fuel_core_storage::{
    Error as StorageError,
    Mappable,
    MerkleRoot,
    Result as StorageResult,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::{
    blockchain::primitives::BlockId,
    fuel_merkle::{
        binary,
        sparse,
    },
    fuel_tx::TxId,
    fuel_types::{
        BlockHeight,
        ContractId,
        Nonce,
    },
};
use serde::{
    de::DeserializeOwned,
    Serialize,
};
use std::{
    borrow::Cow,
    ops::Deref,
};

/// Metadata for dense Merkle trees
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct DenseMerkleMetadata {
    /// The root hash of the dense Merkle tree structure
    pub root: MerkleRoot,
    /// The version of the dense Merkle tree structure is equal to the number of
    /// leaves. Every time we append a new leaf to the Merkle tree data set, we
    /// increment the version number.
    pub version: u64,
}

impl Default for DenseMerkleMetadata {
    fn default() -> Self {
        let empty_merkle_tree = binary::root_calculator::MerkleRootCalculator::new();
        Self {
            root: empty_merkle_tree.root(),
            version: 0,
        }
    }
}

/// Metadata for sparse Merkle trees
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct SparseMerkleMetadata {
    /// The root hash of the sparse Merkle tree structure
    pub root: MerkleRoot,
}

impl Default for SparseMerkleMetadata {
    fn default() -> Self {
        let empty_merkle_tree = sparse::in_memory::MerkleTree::new();
        Self {
            root: empty_merkle_tree.root(),
        }
    }
}

/// The table of fuel block's secondary key - `BlockHeight`.
/// It links the `BlockHeight` to corresponding `BlockId`.
pub struct FuelBlockSecondaryKeyBlockHeights;

impl Mappable for FuelBlockSecondaryKeyBlockHeights {
    /// Secondary key - `BlockHeight`.
    type Key = BlockHeight;
    type OwnedKey = Self::Key;
    /// Primary key - `BlockId`.
    type Value = BlockId;
    type OwnedValue = Self::Value;
}

/// The table of BMT data for Fuel blocks.
pub struct FuelBlockMerkleData;

impl Mappable for FuelBlockMerkleData {
    type Key = u64;
    type OwnedKey = Self::Key;
    type Value = binary::Primitive;
    type OwnedValue = Self::Value;
}

/// The metadata table for [`FuelBlockMerkleData`](FuelBlockMerkleData) table.
pub struct FuelBlockMerkleMetadata;

impl Mappable for FuelBlockMerkleMetadata {
    type Key = BlockHeight;
    type OwnedKey = Self::Key;
    type Value = DenseMerkleMetadata;
    type OwnedValue = Self::Value;
}

/// The table of SMT data for Contract assets.
pub struct ContractsAssetsMerkleData;

impl Mappable for ContractsAssetsMerkleData {
    type Key = [u8; 32];
    type OwnedKey = Self::Key;
    type Value = sparse::Primitive;
    type OwnedValue = Self::Value;
}

/// The metadata table for [`ContractsAssetsMerkleData`](ContractsAssetsMerkleData) table
pub struct ContractsAssetsMerkleMetadata;

impl Mappable for ContractsAssetsMerkleMetadata {
    type Key = ContractId;
    type OwnedKey = Self::Key;
    type Value = SparseMerkleMetadata;
    type OwnedValue = Self::Value;
}

/// The table of SMT data for Contract state.
pub struct ContractsStateMerkleData;

impl Mappable for ContractsStateMerkleData {
    type Key = [u8; 32];
    type OwnedKey = Self::Key;
    type Value = sparse::Primitive;
    type OwnedValue = Self::Value;
}

/// The metadata table for [`ContractsStateMerkleData`](ContractsStateMerkleData) table
pub struct ContractsStateMerkleMetadata;

impl Mappable for ContractsStateMerkleMetadata {
    type Key = ContractId;
    type OwnedKey = Self::Key;
    type Value = SparseMerkleMetadata;
    type OwnedValue = Self::Value;
}

/// The table has a corresponding column in the database.
///
/// Using this trait allows the configured mappable type to have its'
/// database integration auto-implemented for single column interactions.
///
/// If the mappable type requires access to multiple columns or custom logic during setting/getting
/// then its' storage interfaces should be manually implemented and this trait should be avoided.
pub trait DatabaseColumn {
    /// The column of the table.
    fn column() -> Column;
}

impl DatabaseColumn for FuelBlockSecondaryKeyBlockHeights {
    fn column() -> Column {
        Column::FuelBlockSecondaryKeyBlockHeights
    }
}

impl DatabaseColumn for FuelBlockMerkleData {
    fn column() -> Column {
        Column::FuelBlockMerkleData
    }
}

impl DatabaseColumn for FuelBlockMerkleMetadata {
    fn column() -> Column {
        Column::FuelBlockMerkleMetadata
    }
}

impl DatabaseColumn for ContractsAssetsMerkleData {
    fn column() -> Column {
        Column::ContractsAssetsMerkleData
    }
}

impl DatabaseColumn for ContractsAssetsMerkleMetadata {
    fn column() -> Column {
        Column::ContractsAssetsMerkleMetadata
    }
}

impl DatabaseColumn for ContractsStateMerkleData {
    fn column() -> Column {
        Column::ContractsStateMerkleData
    }
}

impl DatabaseColumn for ContractsStateMerkleMetadata {
    fn column() -> Column {
        Column::ContractsStateMerkleMetadata
    }
}

impl<T> StorageInspect<T> for Database
where
    T: Mappable + DatabaseColumn,
    T::Key: ToDatabaseKey,
    T::OwnedValue: DeserializeOwned,
{
    type Error = StorageError;

    fn get(&self, key: &T::Key) -> StorageResult<Option<Cow<T::OwnedValue>>> {
        self.get(key.database_key().as_ref(), T::column())
            .map_err(Into::into)
    }

    fn contains_key(&self, key: &T::Key) -> StorageResult<bool> {
        self.contains_key(key.database_key().as_ref(), T::column())
            .map_err(Into::into)
    }
}

impl<T> StorageMutate<T> for Database
where
    T: Mappable + DatabaseColumn,
    T::Key: ToDatabaseKey,
    T::Value: Serialize,
    T::OwnedValue: DeserializeOwned,
{
    fn insert(
        &mut self,
        key: &T::Key,
        value: &T::Value,
    ) -> StorageResult<Option<T::OwnedValue>> {
        Database::insert(self, key.database_key().as_ref(), T::column(), &value)
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &T::Key) -> StorageResult<Option<T::OwnedValue>> {
        Database::take(self, key.database_key().as_ref(), T::column()).map_err(Into::into)
    }
}

/// Some keys requires pre-processing that could change their type.
pub trait ToDatabaseKey {
    /// A new type of prepared database key that can be converted into bytes.
    type Type<'a>: AsRef<[u8]>
    where
        Self: 'a;

    /// Converts the key into database key that supports byte presentation.
    fn database_key(&self) -> Self::Type<'_>;
}

impl ToDatabaseKey for BlockHeight {
    type Type<'a> = [u8; 4];

    fn database_key(&self) -> Self::Type<'_> {
        self.to_bytes()
    }
}

impl ToDatabaseKey for u64 {
    type Type<'a> = [u8; 8];

    fn database_key(&self) -> Self::Type<'_> {
        self.to_be_bytes()
    }
}

impl ToDatabaseKey for Nonce {
    type Type<'a> = &'a [u8; 32];

    fn database_key(&self) -> Self::Type<'_> {
        self.deref()
    }
}

impl ToDatabaseKey for ContractId {
    type Type<'a> = &'a [u8; 32];

    fn database_key(&self) -> Self::Type<'_> {
        self.deref()
    }
}

impl ToDatabaseKey for BlockId {
    type Type<'a> = &'a [u8];

    fn database_key(&self) -> Self::Type<'_> {
        self.as_slice()
    }
}

impl ToDatabaseKey for TxId {
    type Type<'a> = &'a [u8; 32];

    fn database_key(&self) -> Self::Type<'_> {
        self.deref()
    }
}

impl ToDatabaseKey for () {
    type Type<'a> = &'a [u8];

    fn database_key(&self) -> Self::Type<'_> {
        &[]
    }
}

impl<const N: usize> ToDatabaseKey for [u8; N] {
    type Type<'a> = &'a [u8];

    fn database_key(&self) -> Self::Type<'_> {
        self.as_slice()
    }
}
