use super::Database;
use fuel_core_chain_config::GenesisCommitment;
use fuel_core_executor::refs::ContractRef;
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::postcard::Postcard,
    column::Column,
    structured_storage::TableWithBlueprint,
    tables::{
        Coins,
        ContractsLatestUtxo,
        Messages,
    },
    transactional::Transactional,
    Mappable,
    MerkleRoot,
    Result,
    StorageAsMut,
    StorageInspect,
};
use fuel_core_types::{
    fuel_merkle::binary::root_calculator::MerkleRootCalculator,
    fuel_types::ContractId,
};
use itertools::process_results;
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

    pub fn genesis_coin_root(&self) -> Result<MerkleRoot> {
        let roots_iter = self.iter_all::<Coins>(None);

        let roots = process_results(roots_iter, |roots| {
            roots
                .map(|(_, coin)| coin.root().unwrap())
                .collect::<Vec<MerkleRoot>>()
        })?
        .into_iter();

        Ok(MerkleRootCalculator::new().root_from_iterator(roots))
    }

    pub fn genesis_messages_root(&self) -> Result<MerkleRoot> {
        let roots_iter = self.iter_all::<Messages>(None);

        let roots = process_results(roots_iter, |roots| {
            roots
                .map(|(_, message)| message.root().unwrap())
                .collect::<Vec<MerkleRoot>>()
        })?
        .into_iter();

        Ok(MerkleRootCalculator::new().root_from_iterator(roots))
    }

    pub fn genesis_contracts_root(&self) -> Result<MerkleRoot> {
        let mut database_transaction = Transactional::transaction(self);

        let database = database_transaction.as_mut();
        let contracts_iter = self.iter_all::<ContractsLatestUtxo>(None);

        let roots = process_results(contracts_iter, |contracts| {
            contracts
                .map(|(contract_id, _)| {
                    ContractRef::new(&mut *database, contract_id)
                        .root()
                        .unwrap()
                })
                .collect::<Vec<MerkleRoot>>()
        })?
        .into_iter();

        Ok(MerkleRootCalculator::new().root_from_iterator(roots))
    }
}
