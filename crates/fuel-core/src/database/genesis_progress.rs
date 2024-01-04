use fuel_core_storage::{
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
use itertools::process_results;

use super::{
    storage::{
        GenesisCoinRoots,
        GenesisContractBalanceRoots,
        GenesisContractIds,
        GenesisContractRoots,
        GenesisContractStateRoots,
        GenesisMessageRoots,
        GenesisMetadata,
        ToDatabaseKey,
    },
    Column,
    Database,
};

#[derive(Debug, Clone, Copy)]
pub enum GenesisResource {
    Coins,
    Messages,
    Contracts,
    ContractStates,
    ContractBalances,
}

impl ToDatabaseKey for GenesisResource {
    type Type<'a> = [u8; 1];

    fn database_key(&self) -> Self::Type<'_> {
        [*self as u8]
    }
}

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

    pub fn add_state_root(&mut self, root: MerkleRoot) -> Result<()> {
        StorageMutate::<GenesisContractStateRoots>::insert(self, &root, &())?;
        Ok(())
    }

    pub fn add_balance_root(&mut self, root: MerkleRoot) -> Result<()> {
        StorageMutate::<GenesisContractBalanceRoots>::insert(self, &root, &())?;
        Ok(())
    }

    pub fn add_contract_id(&mut self, contract_id: ContractId) -> Result<()> {
        StorageMutate::<GenesisContractIds>::insert(self, &contract_id, &())?;
        Ok(())
    }

    fn genesis_roots(&self, column: Column) -> Result<MerkleRoot> {
        let roots_iter = self.iter_all::<Vec<u8>, ()>(column, None);

        let roots = process_results(roots_iter, |roots| {
            roots
                .map(|(root, _)| MerkleRoot::try_from(root).unwrap())
                .collect::<Vec<MerkleRoot>>()
        })?
        .into_iter()
        .enumerate()
        .map(|(idx, root)| (MerkleTreeKey::new(idx.to_be_bytes()), root));

        Ok(MerkleTree::root_from_set(roots.into_iter()))
    }

    pub fn genesis_coin_root(&self) -> Result<MerkleRoot> {
        self.genesis_roots(Column::CoinRoots)
    }

    pub fn genesis_messages_root(&self) -> Result<MerkleRoot> {
        self.genesis_roots(Column::MessageRoots)
    }

    pub fn genesis_contracts_root(&self) -> Result<MerkleRoot> {
        self.genesis_roots(Column::ContractRoots)
    }

    pub fn genesis_states_root(&self) -> Result<MerkleRoot> {
        self.genesis_roots(Column::ContractStateRoots)
    }

    pub fn genesis_balances_root(&self) -> Result<MerkleRoot> {
        self.genesis_roots(Column::ContractBalanceRoots)
    }

    pub fn genesis_contract_ids(&self) -> Result<Vec<ContractId>> {
        let contract_ids_iter =
            self.iter_all::<Vec<u8>, ()>(Column::GenesisContractIds, None);

        let contract_ids = process_results(contract_ids_iter, |contract_ids| {
            contract_ids
                .map(|(contract_id, _)| {
                    let bytes32: [u8; 32] = contract_id.try_into().unwrap();
                    ContractId::from(bytes32)
                })
                .collect::<Vec<ContractId>>()
        })?;

        Ok(contract_ids)
    }

    pub fn remove_genesis_progress(&mut self) -> Result<()> {
        todo!("remove columns related to genesis progress tracking");
    }
}
