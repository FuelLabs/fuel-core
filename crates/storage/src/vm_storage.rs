//! The wrapper around the storage for VM implements non-storage getters.

use crate::{
    not_found,
    tables::{
        ContractsAssets,
        ContractsInfo,
        ContractsRawCode,
        ContractsState,
    },
    ContractsAssetsStorage,
    ContractsStateKey,
    Error as StorageError,
    Mappable,
    MerkleRoot,
    MerkleRootStorage,
    StorageAsMut,
    StorageInspect,
    StorageMutate,
    StorageRead,
    StorageSize,
};
use anyhow::anyhow;
use fuel_core_types::{
    blockchain::{
        header::ConsensusHeader,
        primitives::BlockId,
    },
    fuel_tx::{
        Contract,
        StorageSlot,
    },
    fuel_types::{
        BlockHeight,
        Bytes32,
        ContractId,
        Salt,
        Word,
    },
    fuel_vm::InterpreterStorage,
    tai64::Tai64,
};
use primitive_types::U256;
use std::borrow::Cow;

/// Used to store metadata relevant during the execution of a transaction.
#[derive(Clone, Debug)]
pub struct VmStorage<D> {
    current_block_height: BlockHeight,
    current_timestamp: Tai64,
    coinbase: ContractId,
    database: D,
}

/// The trait around the `U256` type allows increasing the key by one.
pub trait IncreaseStorageKey {
    /// Increases the key by one.
    ///
    /// Returns a `Result::Err` in the case of overflow.
    fn increase(&mut self) -> anyhow::Result<()>;
}

impl IncreaseStorageKey for U256 {
    fn increase(&mut self) -> anyhow::Result<()> {
        *self = self
            .checked_add(1.into())
            .ok_or_else(|| anyhow!("range op exceeded available keyspace"))?;
        Ok(())
    }
}

impl<D: Default> Default for VmStorage<D> {
    fn default() -> Self {
        Self {
            current_block_height: Default::default(),
            current_timestamp: Tai64::now(),
            coinbase: Default::default(),
            database: D::default(),
        }
    }
}

impl<D> VmStorage<D> {
    /// Create and instance of the VM storage around the `header` and `coinbase` contract id.
    pub fn new<T>(
        database: D,
        header: &ConsensusHeader<T>,
        coinbase: ContractId,
    ) -> Self {
        Self {
            current_block_height: header.height,
            current_timestamp: header.time,
            coinbase,
            database,
        }
    }

    /// The helper function allows modification of the underlying storage.
    #[cfg(feature = "test-helpers")]
    pub fn database_mut(&mut self) -> &mut D {
        &mut self.database
    }
}

impl<D, M: Mappable> StorageInspect<M> for VmStorage<D>
where
    D: StorageInspect<M, Error = StorageError>,
{
    type Error = StorageError;

    fn get(&self, key: &M::Key) -> Result<Option<Cow<M::OwnedValue>>, Self::Error> {
        StorageInspect::<M>::get(&self.database, key)
    }

    fn contains_key(&self, key: &M::Key) -> Result<bool, Self::Error> {
        StorageInspect::<M>::contains_key(&self.database, key)
    }
}

impl<D, M: Mappable> StorageMutate<M> for VmStorage<D>
where
    D: StorageMutate<M, Error = StorageError>,
{
    fn insert(
        &mut self,
        key: &M::Key,
        value: &M::Value,
    ) -> Result<Option<M::OwnedValue>, Self::Error> {
        StorageMutate::<M>::insert(&mut self.database, key, value)
    }

    fn remove(&mut self, key: &M::Key) -> Result<Option<M::OwnedValue>, Self::Error> {
        StorageMutate::<M>::remove(&mut self.database, key)
    }
}

impl<D, M: Mappable> StorageSize<M> for VmStorage<D>
where
    D: StorageSize<M, Error = StorageError>,
{
    fn size_of_value(&self, key: &M::Key) -> Result<Option<usize>, Self::Error> {
        StorageSize::<M>::size_of_value(&self.database, key)
    }
}

impl<D, M: Mappable> StorageRead<M> for VmStorage<D>
where
    D: StorageRead<M, Error = StorageError>,
{
    fn read(&self, key: &M::Key, buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        StorageRead::<M>::read(&self.database, key, buf)
    }

    fn read_alloc(
        &self,
        key: &<M as Mappable>::Key,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        StorageRead::<M>::read_alloc(&self.database, key)
    }
}

impl<D, K, M: Mappable> MerkleRootStorage<K, M> for VmStorage<D>
where
    D: MerkleRootStorage<K, M, Error = StorageError>,
{
    fn root(&self, key: &K) -> Result<MerkleRoot, Self::Error> {
        MerkleRootStorage::<K, M>::root(&self.database, key)
    }
}

impl<D> ContractsAssetsStorage for VmStorage<D> where
    D: MerkleRootStorage<ContractId, ContractsAssets, Error = StorageError>
{
}

impl<D> InterpreterStorage for VmStorage<D>
where
    D: StorageMutate<ContractsInfo, Error = StorageError>
        + MerkleRootStorage<ContractId, ContractsState, Error = StorageError>
        + StorageMutate<ContractsRawCode, Error = StorageError>
        + StorageRead<ContractsRawCode, Error = StorageError>
        + MerkleRootStorage<ContractId, ContractsAssets, Error = StorageError>
        + VmStorageRequirements<Error = StorageError>,
{
    type DataError = StorageError;

    fn block_height(&self) -> Result<BlockHeight, Self::DataError> {
        Ok(self.current_block_height)
    }

    fn timestamp(&self, height: BlockHeight) -> Result<Word, Self::DataError> {
        let timestamp = match height {
            // panic if $rB is greater than the current block height.
            height if height > self.current_block_height => {
                return Err(anyhow!("block height too high for timestamp").into())
            }
            height if height == self.current_block_height => self.current_timestamp,
            height => self.database.block_time(&height)?,
        };
        Ok(timestamp.0)
    }

    fn block_hash(&self, block_height: BlockHeight) -> Result<Bytes32, Self::DataError> {
        // Block header hashes for blocks with height greater than or equal to current block height are zero (0x00**32).
        // https://github.com/FuelLabs/fuel-specs/blob/master/specs/vm/instruction_set.md#bhsh-block-hash
        if block_height >= self.current_block_height || block_height == Default::default()
        {
            Ok(Bytes32::zeroed())
        } else {
            // this will return 0x00**32 for block height 0 as well
            self.database
                .get_block_id(&block_height)?
                .ok_or(not_found!("BlockId"))
                .map(Into::into)
        }
    }

    fn coinbase(&self) -> Result<ContractId, Self::DataError> {
        Ok(self.coinbase)
    }

    fn deploy_contract_with_id(
        &mut self,
        salt: &Salt,
        slots: &[StorageSlot],
        contract: &Contract,
        root: &Bytes32,
        id: &ContractId,
    ) -> Result<(), Self::DataError> {
        self.storage_contract_insert(id, contract)?;
        self.storage_contract_root_insert(id, salt, root)?;

        self.database.init_contract_state(
            id,
            slots.iter().map(|slot| (*slot.key(), *slot.value())),
        )
    }

    fn merkle_contract_state_range(
        &self,
        contract_id: &ContractId,
        start_key: &Bytes32,
        range: usize,
    ) -> Result<Vec<Option<Cow<Bytes32>>>, Self::DataError> {
        use crate::StorageAsRef;

        let mut key = U256::from_big_endian(start_key.as_ref());
        let mut state_key = Bytes32::zeroed();

        let mut results = Vec::new();
        for _ in 0..range {
            key.to_big_endian(state_key.as_mut());
            let multikey = ContractsStateKey::new(contract_id, &state_key);
            results.push(self.database.storage::<ContractsState>().get(&multikey)?);
            key.increase()?;
        }
        Ok(results)
    }

    fn merkle_contract_state_insert_range(
        &mut self,
        contract_id: &ContractId,
        start_key: &Bytes32,
        values: &[Bytes32],
    ) -> Result<usize, Self::DataError> {
        let mut current_key = U256::from_big_endian(start_key.as_ref());
        // verify key is in range
        current_key
            .checked_add(U256::from(values.len()))
            .ok_or_else(|| anyhow!("range op exceeded available keyspace"))?;

        let mut key_bytes = Bytes32::zeroed();
        let mut found_unset = 0u32;
        for value in values {
            current_key.to_big_endian(key_bytes.as_mut());

            let option = self
                .database
                .storage::<ContractsState>()
                .insert(&(contract_id, &key_bytes).into(), value)?;

            if option.is_none() {
                found_unset = found_unset
                    .checked_add(1)
                    .expect("We've checked it above via `values.len()`");
            }

            current_key.increase()?;
        }

        Ok(found_unset as usize)
    }

    fn merkle_contract_state_remove_range(
        &mut self,
        contract_id: &ContractId,
        start_key: &Bytes32,
        range: usize,
    ) -> Result<Option<()>, Self::DataError> {
        let mut found_unset = false;

        let mut current_key = U256::from_big_endian(start_key.as_ref());

        let mut key_bytes = Bytes32::zeroed();
        for _ in 0..range {
            current_key.to_big_endian(key_bytes.as_mut());

            let option = self
                .database
                .storage::<ContractsState>()
                .remove(&(contract_id, &key_bytes).into())?;

            found_unset |= option.is_none();

            current_key.increase()?;
        }

        if found_unset {
            Ok(None)
        } else {
            Ok(Some(()))
        }
    }
}

/// The requirements for the storage for optimal work of the [`VmStorage`].
pub trait VmStorageRequirements {
    /// The error used by the storage.
    type Error;

    /// Returns a block time based on the block `height`.
    fn block_time(&self, height: &BlockHeight) -> Result<Tai64, Self::Error>;

    /// Returns the `BlockId` for the block at `height`.
    fn get_block_id(&self, height: &BlockHeight) -> Result<Option<BlockId>, Self::Error>;

    /// Initialize the contract state with a batch of the key/value pairs.
    fn init_contract_state<S: Iterator<Item = (Bytes32, Bytes32)>>(
        &mut self,
        contract_id: &ContractId,
        slots: S,
    ) -> Result<(), Self::Error>;
}
