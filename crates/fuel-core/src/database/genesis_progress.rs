use crate::graphql_api::storage::Column as OffChainColumn;

use super::{
    database_description::{
        off_chain::OffChain,
        on_chain::OnChain,
        DatabaseDescription,
    },
    GenesisDatabase,
};
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
        ProcessedTransactions,
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

pub struct GenesisMetadata<Description>(core::marker::PhantomData<Description>);

impl<Description> Mappable for GenesisMetadata<Description> {
    type Key = str;
    type OwnedKey = String;
    type Value = Self::OwnedValue;
    type OwnedValue = usize;
}

impl TableWithBlueprint for GenesisMetadata<OnChain> {
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = <OnChain as DatabaseDescription>::Column;
    fn column() -> Self::Column {
        Column::GenesisMetadata
    }
}

impl TableWithBlueprint for GenesisMetadata<OffChain> {
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = <OffChain as DatabaseDescription>::Column;
    fn column() -> Self::Column {
        OffChainColumn::GenesisMetadata
    }
}

pub trait GenesisProgressInspect<Description> {
    fn genesis_progress(&self, key: &str) -> Option<usize>;
}

pub trait GenesisProgressMutate<Description> {
    fn update_genesis_progress(
        &mut self,
        key: &str,
        processed_group: usize,
    ) -> Result<()>;
}

impl<S, DbDesc> GenesisProgressInspect<DbDesc> for S
where
    S: StorageInspect<GenesisMetadata<DbDesc>, Error = StorageError>,
    DbDesc: DatabaseDescription,
{
    fn genesis_progress(
        &self,
        key: &<GenesisMetadata<DbDesc> as Mappable>::Key,
    ) -> Option<usize> {
        Some(
            StorageInspect::<GenesisMetadata<DbDesc>>::get(self, key)
                .ok()??
                .into_owned(),
        )
    }
}

impl<S, DbDesc> GenesisProgressMutate<DbDesc> for S
where
    S: StorageMutate<GenesisMetadata<DbDesc>, Error = StorageError>,
{
    fn update_genesis_progress(
        &mut self,
        key: &<GenesisMetadata<DbDesc> as Mappable>::Key,
        processed_group: usize,
    ) -> Result<()> {
        self.storage_as_mut::<GenesisMetadata<DbDesc>>()
            .insert(key, &processed_group)?;

        Ok(())
    }
}

impl GenesisDatabase {
    pub fn genesis_coins_root(&self) -> Result<MerkleRoot> {
        let coins = self.iter_all::<Coins>(None);

        let mut root_calculator = MerkleRootCalculator::new();
        for coin in coins {
            let (utxo_id, coin) = coin?;
            root_calculator.push(coin.uncompress(utxo_id).root()?.as_slice());
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

    pub fn processed_transactions_root(&self) -> Result<MerkleRoot> {
        let txs = self.iter_all::<ProcessedTransactions>(None);

        let mut root_calculator = MerkleRootCalculator::new();
        for tx in txs {
            let (tx_id, _) = tx?;
            root_calculator.push(tx_id.as_slice());
        }

        Ok(root_calculator.root())
    }
}
