use fuel_core_chain_config::TableEntry;
use fuel_core_storage::{
    tables::{
        merkle::ContractsAssetsMerkleMetadata,
        ContractsAssets,
    },
    ContractsAssetKey,
    Error as StorageError,
    StorageAsRef,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::{
    fuel_asm::Word,
    fuel_types::{
        AssetId,
        ContractId,
    },
};
use itertools::Itertools;

pub trait BalancesInitializer {
    /// Initialize the balances of the contract from the all leaves.
    /// This method is more performant than inserting balances one by one.
    fn init_contract_balances<S>(
        &mut self,
        contract_id: &ContractId,
        balances: S,
    ) -> Result<(), StorageError>
    where
        S: Iterator<Item = (AssetId, Word)>;

    fn update_contract_balances(
        &mut self,
        balances: impl IntoIterator<Item = TableEntry<ContractsAssets>>,
    ) -> Result<(), StorageError>;
}

impl<S> BalancesInitializer for S
where
    S: StorageInspect<ContractsAssetsMerkleMetadata, Error = StorageError>,
    S: StorageMutate<ContractsAssets, Error = StorageError>,
{
    fn init_contract_balances<I>(
        &mut self,
        contract_id: &ContractId,
        balances: I,
    ) -> Result<(), StorageError>
    where
        I: Iterator<Item = (AssetId, Word)>,
    {
        let balances = balances
            .map(|(asset, balance)| {
                (ContractsAssetKey::new(contract_id, &asset), balance)
            })
            .collect_vec();
        for (key, value) in balances.iter() {
            self.insert(key, value)?;
        }
        Ok(())
    }

    fn update_contract_balances(
        &mut self,
        balances: impl IntoIterator<Item = TableEntry<ContractsAssets>>,
    ) -> Result<(), StorageError> {
        balances
            .into_iter()
            .group_by(|s| *s.key.contract_id())
            .into_iter()
            .try_for_each(|(contract_id, entries)| {
                if self
                    .storage::<ContractsAssetsMerkleMetadata>()
                    .get(&contract_id)?
                    .is_some()
                {
                    let balance_entries = entries
                        .into_iter()
                        .map(|balance_entry| (balance_entry.key, balance_entry.value))
                        .collect_vec();

                    #[allow(clippy::map_identity)]
                    let balance_entries_iter =
                        balance_entries.iter().map(|(key, value)| (key, value));

                    for (key, value) in balance_entries_iter {
                        self.insert(key, value)?;
                    }
                    Ok(())
                } else {
                    self.init_contract_balances(
                        &contract_id,
                        entries.into_iter().map(|e| (*e.key.asset_id(), e.value)),
                    )
                }
            })?;

        Ok(())
    }
}
