use super::Database;
use fuel_core_chain_config::GenesisCommitment;
use fuel_core_executor::refs::ContractRef;
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::postcard::Postcard,
    column::Column,
    iter::IteratorOverTable,
    structured_storage::TableWithBlueprint,
    tables::{
        Coins,
        ContractsLatestUtxo,
        Messages,
    },
    Error as StorageError,
    Mappable,
    MerkleRoot,
    Result,
    StorageAsMut,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::fuel_merkle::binary::root_calculator::MerkleRootCalculator;
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

pub trait GenesisProgressInspect {
    fn genesis_progress(&self, key: &GenesisResource) -> Option<usize>;
}

pub trait GenesisProgressMutate {
    fn update_genesis_progress(
        &mut self,
        key: GenesisResource,
        processed_group: usize,
    ) -> Result<()>;
}

impl<S> GenesisProgressInspect for S
where
    S: StorageInspect<GenesisMetadata, Error = StorageError>,
{
    fn genesis_progress(&self, key: &GenesisResource) -> Option<usize> {
        Some(
            StorageInspect::<GenesisMetadata>::get(self, key)
                .ok()??
                .into_owned(),
        )
    }
}

impl<S> GenesisProgressMutate for S
where
    S: StorageMutate<GenesisMetadata, Error = StorageError>,
{
    fn update_genesis_progress(
        &mut self,
        key: GenesisResource,
        processed_group: usize,
    ) -> Result<()> {
        self.storage_as_mut::<GenesisMetadata>()
            .insert(&key, &processed_group)?;

        Ok(())
    }
}

impl Database {
    pub fn genesis_coins_root(&self) -> Result<MerkleRoot> {
        let coins = self.iter_all::<Coins>(None);

        let mut root_calculator = MerkleRootCalculator::new();
        for coin in coins {
            let (_, coin) = coin?;
            root_calculator.push(coin.root()?.as_slice());
        }

        Ok(root_calculator.root())
    }

    pub fn genesis_messages_root(&self) -> Result<MerkleRoot> {
        let messages = self.iter_all::<Messages>(None);

        let mut root_calculator = MerkleRootCalculator::new();
        for message in messages {
            let (_, message) = message?;
            root_calculator.push(message.root()?.as_slice());
        }

        Ok(root_calculator.root())
    }

    pub fn genesis_contracts_root(&self) -> Result<MerkleRoot> {
        let contracts = self.iter_all::<ContractsLatestUtxo>(None);

        let mut root_calculator = MerkleRootCalculator::new();
        for contract in contracts {
            let (contract_id, _) = contract?;
            let root = ContractRef::new(self, contract_id).root()?;
            root_calculator.push(root.as_slice());
        }

        Ok(root_calculator.root())
    }
}
