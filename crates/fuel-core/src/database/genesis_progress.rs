use crate::state::DataSource;

use super::Database;
use fuel_core_storage::{
    blueprint::{
        plain::Plain,
        Blueprint,
    },
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    column::Column,
    structured_storage::TableWithBlueprint,
    tables::ContractsLatestUtxo,
    Mappable,
    MerkleRoot,
    Result,
    StorageAsMut,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::{
    fuel_merkle::sparse::{
        in_memory::MerkleTree,
        MerkleTreeKey,
    },
    fuel_types::ContractId,
};
use itertools::{
    process_results,
    Itertools,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Copy, strum::EnumIter, Serialize, Deserialize)]
pub enum GenesisResource {
    Coins,
    Messages,
    Contracts,
    ContractStates,
    ContractBalances,
    ContractsRoot,
}

pub struct GenesisMetadata;

impl Mappable for GenesisMetadata {
    type Key = Self::OwnedKey;
    type OwnedKey = GenesisResource;
    type Value = Self::OwnedValue;
    type OwnedValue = usize;
}

impl TableWithBlueprint for GenesisMetadata {
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = Column;
    fn column() -> Self::Column {
        Column::GenesisMetadata
    }
}

// TODO: Remove as part of the https://github.com/FuelLabs/fuel-core/issues/1734
pub struct GenesisCoinRoots;

impl Mappable for GenesisCoinRoots {
    type Key = Self::OwnedKey;
    type OwnedKey = MerkleRoot;
    type Value = Self::OwnedValue;
    type OwnedValue = ();
}

impl TableWithBlueprint for GenesisCoinRoots {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = Column;
    fn column() -> Self::Column {
        Column::GenesisCoinRoots
    }
}

// TODO: Remove as part of the https://github.com/FuelLabs/fuel-core/issues/1734
pub struct GenesisMessageRoots;

impl Mappable for GenesisMessageRoots {
    type Key = Self::OwnedKey;
    type OwnedKey = MerkleRoot;
    type Value = Self::OwnedValue;
    type OwnedValue = ();
}

impl TableWithBlueprint for GenesisMessageRoots {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = Column;
    fn column() -> Self::Column {
        Column::GenesisMessageRoots
    }
}

// TODO: Remove as part of the https://github.com/FuelLabs/fuel-core/issues/1734
pub struct GenesisContractRoots;

impl Mappable for GenesisContractRoots {
    type Key = Self::OwnedKey;
    type OwnedKey = MerkleRoot;
    type Value = Self::OwnedValue;
    type OwnedValue = ();
}

impl TableWithBlueprint for GenesisContractRoots {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = Column;
    fn column() -> Self::Column {
        Column::GenesisContractRoots
    }
}

pub type GenesisImportedContractId = ContractId;

impl Database {
    pub fn genesis_progress(&self, key: &GenesisResource) -> Option<usize> {
        Some(
            StorageInspect::<GenesisMetadata>::get(self, key)
                .unwrap()?
                .into_owned(),
        )
    }

    pub fn update_genesis_progress(
        &mut self,
        key: GenesisResource,
        processed_group: usize,
    ) -> Result<()> {
        self.storage_as_mut::<GenesisMetadata>()
            .insert(&key, &processed_group)?;

        Ok(())
    }

    pub fn add_coin_root(&mut self, root: MerkleRoot) -> Result<()> {
        StorageMutate::<GenesisCoinRoots>::insert(self, &root, &())?;
        Ok(())
    }

    pub fn add_message_root(&mut self, root: MerkleRoot) -> Result<()> {
        StorageMutate::<GenesisMessageRoots>::insert(self, &root, &())?;
        Ok(())
    }

    pub fn add_contract_root(&mut self, root: MerkleRoot) -> Result<()> {
        StorageMutate::<GenesisContractRoots>::insert(self, &root, &())?;
        Ok(())
    }

    pub(crate) fn genesis_roots<M>(
        &self,
    ) -> Result<impl Iterator<Item = (MerkleTreeKey, [u8; 32])>>
    where
        M: Mappable<OwnedKey = [u8; 32]> + TableWithBlueprint<Column = Column>,
        M::Blueprint: Blueprint<M, DataSource>,
    {
        let roots_iter = self.iter_all::<M>(None);

        let roots = process_results(roots_iter, |roots| {
            roots.map(|(root, _)| root).collect::<Vec<MerkleRoot>>()
        })?
        .into_iter()
        .enumerate()
        .map(|(idx, root)| (MerkleTreeKey::new(idx.to_be_bytes()), root));

        Ok(roots)
    }

    fn compute_genesis_root<M>(&self) -> Result<MerkleRoot>
    where
        M: Mappable<OwnedKey = [u8; 32]> + TableWithBlueprint<Column = Column>,
        M::Blueprint: Blueprint<M, DataSource>,
    {
        let roots = self.genesis_roots::<M>()?;
        Ok(MerkleTree::root_from_set(roots.into_iter()))
    }

    pub fn genesis_coin_root(&self) -> Result<MerkleRoot> {
        self.compute_genesis_root::<GenesisCoinRoots>()
    }

    pub fn genesis_messages_root(&self) -> Result<MerkleRoot> {
        self.compute_genesis_root::<GenesisMessageRoots>()
    }

    pub fn genesis_contracts_root(&self) -> Result<MerkleRoot> {
        self.compute_genesis_root::<GenesisContractRoots>()
    }

    pub fn genesis_loaded_contracts(
        &self,
    ) -> impl Iterator<Item = Result<GenesisImportedContractId>> + '_ {
        self.iter_all::<ContractsLatestUtxo>(None)
            .map_ok(|(contract_id, _)| contract_id)
            .map(|res| res.map_err(Into::into))
    }
}
