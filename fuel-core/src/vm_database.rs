use crate::{
    database::{
        transactional::DatabaseTransaction,
        Database,
    },
    state::{
        in_memory::transaction::MemoryTransactionView,
        DataSource,
        Error,
    },
};
pub use fuel_core_interfaces::db::KvStoreError;
use fuel_core_interfaces::{
    common::{
        fuel_asm::Word,
        fuel_storage::{
            Mappable,
            MerkleRoot,
            MerkleRootStorage,
            StorageInspect,
            StorageMutate,
        },
        fuel_tx::{
            field::Outputs,
            AssetId,
            Contract,
            ContractId,
            Output,
            Salt,
        },
        fuel_vm::prelude::{
            Address,
            Bytes32,
            InterpreterStorage,
        },
    },
    db::{
        ContractsAssets,
        ContractsInfo,
        ContractsRawCode,
        ContractsState,
    },
    model::FuelBlock,
};
use std::{
    borrow::Cow,
    fmt::Debug,
    sync::Arc,
};

#[derive(Clone, Debug)]
pub struct VMDatabase<'a> {
    block: &'a FuelBlock,
    database: DatabaseTransaction,
}

impl<'a> VMDatabase<'a> {
    pub fn new(block: &'a FuelBlock, database: DatabaseTransaction) -> Self {
        Self { block, database }
    }

    fn database<Type>(&self) -> &impl StorageInspect<Type, Error = Error>
    where
        Type: Mappable,
        Database: StorageInspect<Type, Error = Error>,
    {
        self.database.as_ref()
    }

    fn database_mut<Type>(&mut self) -> &mut impl StorageMutate<Type, Error = Error>
    where
        Type: Mappable,
        Database: StorageMutate<Type, Error = Error>,
    {
        self.database.as_mut()
    }
}

impl<'a> AsRef<Database> for VMDatabase<'a> {
    fn as_ref(&self) -> &Database {
        &self.database
    }
}

impl<'a> StorageInspect<ContractsRawCode> for VMDatabase<'a> {
    type Error = Error;

    fn get(&self, key: &ContractId) -> Result<Option<Cow<Contract>>, Self::Error> {
        self.database::<ContractsRawCode>().get(key)
    }

    fn contains_key(&self, key: &ContractId) -> Result<bool, Self::Error> {
        self.database::<ContractsRawCode>().contains_key(key)
    }
}

impl<'a> StorageMutate<ContractsRawCode> for VMDatabase<'a> {
    fn insert(
        &mut self,
        key: &ContractId,
        value: &[u8],
    ) -> Result<Option<Contract>, Self::Error> {
        self.database_mut::<ContractsRawCode>().insert(key, value)
    }

    fn remove(&mut self, key: &ContractId) -> Result<Option<Contract>, Self::Error> {
        self.database_mut::<ContractsRawCode>().remove(key)
    }
}

impl<'a> StorageInspect<ContractsInfo> for VMDatabase<'a> {
    type Error = Error;

    fn get(&self, key: &ContractId) -> Result<Option<Cow<(Salt, Bytes32)>>, Self::Error> {
        self.database::<ContractsInfo>().get(key)
    }

    fn contains_key(&self, key: &ContractId) -> Result<bool, Self::Error> {
        self.database::<ContractsInfo>().contains_key(key)
    }
}

impl<'a> StorageMutate<ContractsInfo> for VMDatabase<'a> {
    fn insert(
        &mut self,
        key: &ContractId,
        value: &(Salt, Bytes32),
    ) -> Result<Option<(Salt, Bytes32)>, Self::Error> {
        self.database_mut::<ContractsInfo>().insert(key, value)
    }

    fn remove(
        &mut self,
        key: &ContractId,
    ) -> Result<Option<(Salt, Bytes32)>, Self::Error> {
        self.database_mut::<ContractsInfo>().remove(key)
    }
}

impl<'a> StorageInspect<ContractsAssets<'_>> for VMDatabase<'a> {
    type Error = Error;

    fn get(
        &self,
        key: &(&ContractId, &AssetId),
    ) -> Result<Option<Cow<Word>>, Self::Error> {
        <Database as StorageInspect<ContractsAssets>>::get(&self.database, key)
    }

    fn contains_key(&self, key: &(&ContractId, &AssetId)) -> Result<bool, Self::Error> {
        self.database::<ContractsAssets>().contains_key(key)
    }
}

impl<'a> StorageMutate<ContractsAssets<'_>> for VMDatabase<'a> {
    fn insert(
        &mut self,
        key: &(&ContractId, &AssetId),
        value: &Word,
    ) -> Result<Option<Word>, Self::Error> {
        self.database_mut::<ContractsAssets>().insert(key, value)
    }

    fn remove(
        &mut self,
        key: &(&ContractId, &AssetId),
    ) -> Result<Option<Word>, Self::Error> {
        self.database_mut::<ContractsAssets>().remove(key)
    }
}

impl<'a> MerkleRootStorage<ContractId, ContractsAssets<'_>> for VMDatabase<'a> {
    fn root(&mut self, key: &ContractId) -> Result<MerkleRoot, Self::Error> {
        <Database as MerkleRootStorage<ContractId, ContractsAssets>>::root(
            &mut self.database,
            key,
        )
    }
}

impl<'a> StorageInspect<ContractsState<'_>> for VMDatabase<'a> {
    type Error = Error;

    fn get(
        &self,
        key: &(&ContractId, &Bytes32),
    ) -> Result<Option<Cow<Bytes32>>, Self::Error> {
        <Database as StorageInspect<ContractsState>>::get(&self.database, key)
    }

    fn contains_key(&self, key: &(&ContractId, &Bytes32)) -> Result<bool, Self::Error> {
        self.database::<ContractsState>().contains_key(key)
    }
}

impl<'a> StorageMutate<ContractsState<'_>> for VMDatabase<'a> {
    fn insert(
        &mut self,
        key: &(&ContractId, &Bytes32),
        value: &Bytes32,
    ) -> Result<Option<Bytes32>, Self::Error> {
        self.database_mut::<ContractsState>().insert(key, value)
    }

    fn remove(
        &mut self,
        key: &(&ContractId, &Bytes32),
    ) -> Result<Option<Bytes32>, Self::Error> {
        self.database_mut::<ContractsState>().remove(key)
    }
}

impl<'a> MerkleRootStorage<ContractId, ContractsState<'_>> for VMDatabase<'a> {
    fn root(&mut self, key: &ContractId) -> Result<MerkleRoot, Self::Error> {
        <Database as MerkleRootStorage<ContractId, ContractsState>>::root(
            &mut self.database,
            key,
        )
    }
}

impl<'a> InterpreterStorage for VMDatabase<'a> {
    type DataError = Error;

    fn block_height(&self) -> Result<u32, Error> {
        let height = *self.block.header().height();
        Ok(height.into())
    }

    fn timestamp(&self, height: u32) -> Result<Word, Self::DataError> {
        let millis = if self.block_height()? == height {
            self.block.header().time().to_owned().timestamp_millis()
        } else {
            self.database.block_time(height)?.timestamp_millis()
        };

        millis
            .try_into()
            .map_err(|e| Self::DataError::DatabaseError(Box::new(e)))
    }

    fn block_hash(&self, block_height: u32) -> Result<Bytes32, Error> {
        let hash = self
            .database
            .get_block_id(block_height.into())?
            .unwrap_or_default();
        Ok(hash)
    }

    fn coinbase(&self) -> Result<Address, Error> {
        let coinbase_tx = self
            .block
            .transactions()
            .get(0)
            .ok_or(KvStoreError::NotFound)?;

        let coin = coinbase_tx
            .as_mint()
            .ok_or(KvStoreError::NotFound)?
            .outputs()
            .first()
            .ok_or(KvStoreError::NotFound)?;

        if let Output::Coin { to, .. } = coin {
            Ok(*to)
        } else {
            Err(KvStoreError::NotFound.into())
        }
    }

    fn merkle_contract_state_range(
        &self,
        _id: &ContractId,
        _start_key: &Bytes32,
        _range: Word,
    ) -> Result<Vec<Option<Cow<Bytes32>>>, Self::DataError> {
        unimplemented!()
    }

    fn merkle_contract_state_insert_range(
        &mut self,
        _contract: &ContractId,
        _start_key: &Bytes32,
        _values: &[Bytes32],
    ) -> Result<Option<()>, Self::DataError> {
        unimplemented!()
    }

    fn merkle_contract_state_remove_range(
        &mut self,
        _contract: &ContractId,
        _start_key: &Bytes32,
        _range: Word,
    ) -> Result<Option<()>, Self::DataError> {
        unimplemented!()
    }
}
