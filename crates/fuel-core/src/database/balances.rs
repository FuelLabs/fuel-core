use crate::database::Database;
use fuel_core_storage::{
    tables::ContractsAssets,
    ContractsAssetKey,
    Error as StorageError,
    StorageBatchMutate,
};
use fuel_core_types::{
    fuel_asm::Word,
    fuel_types::{
        AssetId,
        ContractId,
    },
};
use itertools::Itertools;

impl Database {
    /// Initialize the balances of the contract from the all leafs.
    /// This method is more performant than inserting balances one by one.
    pub fn init_contract_balances<S>(
        &mut self,
        contract_id: &ContractId,
        balances: S,
    ) -> Result<(), StorageError>
    where
        S: Iterator<Item = (AssetId, Word)>,
    {
        let balances = balances
            .map(|(asset, balance)| {
                (ContractsAssetKey::new(contract_id, &asset), balance)
            })
            .collect_vec();
        #[allow(clippy::map_identity)]
        <_ as StorageBatchMutate<ContractsAssets>>::init_storage(
            &mut self.data,
            &mut balances.iter().map(|(key, value)| (key, value)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::database_description::on_chain::OnChain;
    use fuel_core_storage::StorageAsMut;
    use fuel_core_types::fuel_types::AssetId;
    use rand::Rng;

    fn random_asset_id<R>(rng: &mut R) -> AssetId
    where
        R: Rng + ?Sized,
    {
        let mut bytes = [0u8; 32];
        rng.fill(bytes.as_mut());
        bytes.into()
    }

    #[test]
    fn init_contract_balances_works() {
        use rand::{
            rngs::StdRng,
            RngCore,
            SeedableRng,
        };

        let rng = &mut StdRng::seed_from_u64(1234);
        let gen = || Some((random_asset_id(rng), rng.next_u64()));
        let data = core::iter::from_fn(gen).take(5_000).collect::<Vec<_>>();

        let contract_id = ContractId::from([1u8; 32]);
        let init_database = &mut Database::default();

        init_database
            .init_contract_balances(&contract_id, data.clone().into_iter())
            .expect("Should init contract");
        let init_root = init_database
            .storage::<ContractsAssets>()
            .root(&contract_id)
            .expect("Should get root");

        let seq_database = &mut Database::<OnChain>::default();
        for (asset, value) in data.iter() {
            seq_database
                .storage::<ContractsAssets>()
                .insert(&ContractsAssetKey::new(&contract_id, asset), value)
                .expect("Should insert a state");
        }
        let seq_root = seq_database
            .storage::<ContractsAssets>()
            .root(&contract_id)
            .expect("Should get root");

        assert_eq!(init_root, seq_root);

        for (asset, value) in data.into_iter() {
            let init_value = init_database
                .storage::<ContractsAssets>()
                .get(&ContractsAssetKey::new(&contract_id, &asset))
                .expect("Should get a state from init database")
                .unwrap()
                .into_owned();
            let seq_value = seq_database
                .storage::<ContractsAssets>()
                .get(&ContractsAssetKey::new(&contract_id, &asset))
                .expect("Should get a state from seq database")
                .unwrap()
                .into_owned();
            assert_eq!(init_value, value);
            assert_eq!(seq_value, value);
        }
    }
}
