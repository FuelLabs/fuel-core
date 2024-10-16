//! The wrapper around the storage for VM implements non-storage getters.

use crate::{
    not_found,
    tables::{
        ConsensusParametersVersions,
        ContractsAssets,
        ContractsRawCode,
        ContractsState,
        FuelBlocks,
        StateTransitionBytecodeVersions,
    },
    ContractsAssetsStorage,
    ContractsStateKey,
    Error as StorageError,
    Mappable,
    MerkleRoot,
    MerkleRootStorage,
    StorageAsMut,
    StorageBatchMutate,
    StorageInspect,
    StorageMutate,
    StorageRead,
    StorageSize,
};
use anyhow::anyhow;
use fuel_core_types::{
    blockchain::{
        header::{
            ApplicationHeader,
            ConsensusHeader,
            ConsensusParametersVersion,
            StateTransitionBytecodeVersion,
        },
        primitives::BlockId,
    },
    fuel_tx::{
        ConsensusParameters,
        Contract,
        StorageSlot,
    },
    fuel_types::{
        BlockHeight,
        Bytes32,
        ContractId,
        Word,
    },
    fuel_vm::InterpreterStorage,
    tai64::Tai64,
};
use fuel_vm_private::{
    fuel_storage::StorageWrite,
    storage::{
        BlobData,
        ContractsStateData,
        UploadedBytecodes,
    },
};
use itertools::Itertools;
use primitive_types::U256;

#[cfg(feature = "std")]
use std::borrow::Cow;

#[cfg(not(feature = "std"))]
use alloc::borrow::Cow;

#[cfg(feature = "alloc")]
use alloc::{
    borrow::ToOwned,
    vec::Vec,
};

/// Used to store metadata relevant during the execution of a transaction.
#[derive(Clone, Debug)]
pub struct VmStorage<D> {
    consensus_parameters_version: ConsensusParametersVersion,
    state_transition_version: StateTransitionBytecodeVersion,
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
            consensus_parameters_version: Default::default(),
            state_transition_version: Default::default(),
            current_block_height: Default::default(),
            current_timestamp: Tai64::now(),
            coinbase: Default::default(),
            database: D::default(),
        }
    }
}

impl<D> VmStorage<D> {
    /// Create and instance of the VM storage around the `header` and `coinbase` contract id.
    pub fn new<C, A>(
        database: D,
        consensus: &ConsensusHeader<C>,
        application: &ApplicationHeader<A>,
        coinbase: ContractId,
    ) -> Self {
        Self {
            consensus_parameters_version: application.consensus_parameters_version,
            state_transition_version: application.state_transition_bytecode_version,
            current_block_height: consensus.height,
            current_timestamp: consensus.time,
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
    fn insert(&mut self, key: &M::Key, value: &M::Value) -> Result<(), Self::Error> {
        StorageMutate::<M>::insert(&mut self.database, key, value)
    }

    fn replace(
        &mut self,
        key: &M::Key,
        value: &M::Value,
    ) -> Result<Option<M::OwnedValue>, Self::Error> {
        StorageMutate::<M>::replace(&mut self.database, key, value)
    }

    fn remove(&mut self, key: &M::Key) -> Result<(), Self::Error> {
        StorageMutate::<M>::remove(&mut self.database, key)
    }

    fn take(&mut self, key: &M::Key) -> Result<Option<M::OwnedValue>, Self::Error> {
        StorageMutate::<M>::take(&mut self.database, key)
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

impl<D, M: Mappable> StorageWrite<M> for VmStorage<D>
where
    D: StorageWrite<M, Error = StorageError>,
{
    fn write_bytes(&mut self, key: &M::Key, buf: &[u8]) -> Result<usize, Self::Error> {
        StorageWrite::<M>::write_bytes(&mut self.database, key, buf)
    }

    fn replace_bytes(
        &mut self,
        key: &M::Key,
        buf: &[u8],
    ) -> Result<(usize, Option<Vec<u8>>), Self::Error> {
        StorageWrite::<M>::replace_bytes(&mut self.database, key, buf)
    }

    fn take_bytes(&mut self, key: &M::Key) -> Result<Option<Vec<u8>>, Self::Error> {
        StorageWrite::<M>::take_bytes(&mut self.database, key)
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
    D: StorageMutate<ContractsAssets, Error = StorageError>
{
}

impl<D> InterpreterStorage for VmStorage<D>
where
    D: StorageWrite<ContractsRawCode, Error = StorageError>
        + StorageSize<ContractsRawCode, Error = StorageError>
        + StorageRead<ContractsRawCode, Error = StorageError>
        + StorageMutate<ContractsAssets, Error = StorageError>
        + StorageWrite<ContractsState, Error = StorageError>
        + StorageSize<ContractsState, Error = StorageError>
        + StorageRead<ContractsState, Error = StorageError>
        + StorageMutate<ConsensusParametersVersions, Error = StorageError>
        + StorageMutate<StateTransitionBytecodeVersions, Error = StorageError>
        + StorageMutate<UploadedBytecodes, Error = StorageError>
        + StorageWrite<BlobData, Error = StorageError>
        + StorageSize<BlobData, Error = StorageError>
        + StorageRead<BlobData, Error = StorageError>
        + VmStorageRequirements<Error = StorageError>,
{
    type DataError = StorageError;

    fn block_height(&self) -> Result<BlockHeight, Self::DataError> {
        Ok(self.current_block_height)
    }

    fn consensus_parameters_version(&self) -> Result<u32, Self::DataError> {
        Ok(self.consensus_parameters_version)
    }

    fn state_transition_version(&self) -> Result<u32, Self::DataError> {
        Ok(self.state_transition_version)
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

    fn set_consensus_parameters(
        &mut self,
        version: u32,
        consensus_parameters: &ConsensusParameters,
    ) -> Result<Option<ConsensusParameters>, Self::DataError> {
        self.database
            .storage_as_mut::<ConsensusParametersVersions>()
            .replace(&version, consensus_parameters)
    }

    fn set_state_transition_bytecode(
        &mut self,
        version: u32,
        hash: &Bytes32,
    ) -> Result<Option<Bytes32>, Self::DataError> {
        self.database
            .storage_as_mut::<StateTransitionBytecodeVersions>()
            .replace(&version, hash)
    }

    fn deploy_contract_with_id(
        &mut self,
        slots: &[StorageSlot],
        contract: &Contract,
        id: &ContractId,
    ) -> Result<(), Self::DataError> {
        self.storage_contract_insert(id, contract)?;
        self.database.init_contract_state(
            id,
            slots.iter().map(|slot| (*slot.key(), *slot.value())),
        )
    }

    fn contract_state_range(
        &self,
        contract_id: &ContractId,
        start_key: &Bytes32,
        range: usize,
    ) -> Result<Vec<Option<Cow<ContractsStateData>>>, Self::DataError> {
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

    fn contract_state_insert_range<'a, I>(
        &mut self,
        contract_id: &ContractId,
        start_key: &Bytes32,
        values: I,
    ) -> Result<usize, Self::DataError>
    where
        I: Iterator<Item = &'a [u8]>,
    {
        let values: Vec<_> = values.collect();
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
                .replace(&(contract_id, &key_bytes).into(), value)?;

            if option.is_none() {
                found_unset = found_unset
                    .checked_add(1)
                    .expect("We've checked it above via `values.len()`");
            }

            current_key.increase()?;
        }

        Ok(found_unset as usize)
    }

    fn contract_state_remove_range(
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
                .take(&(contract_id, &key_bytes).into())?;

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

impl<T> VmStorageRequirements for T
where
    T: StorageInspect<FuelBlocks, Error = StorageError>,
    T: StorageBatchMutate<ContractsState, Error = StorageError>,
{
    type Error = StorageError;

    fn block_time(&self, height: &BlockHeight) -> Result<Tai64, Self::Error> {
        use crate::StorageAsRef;

        let block = self
            .storage::<FuelBlocks>()
            .get(height)?
            .ok_or(not_found!(FuelBlocks))?;
        Ok(block.header().time().to_owned())
    }

    fn get_block_id(&self, height: &BlockHeight) -> Result<Option<BlockId>, Self::Error> {
        use crate::StorageAsRef;

        self.storage::<FuelBlocks>()
            .get(height)
            .map(|v| v.map(|v| v.id()))
    }

    fn init_contract_state<S: Iterator<Item = (Bytes32, Bytes32)>>(
        &mut self,
        contract_id: &ContractId,
        slots: S,
    ) -> Result<(), Self::Error> {
        let slots = slots
            .map(|(key, value)| (ContractsStateKey::new(contract_id, &key), value))
            .collect_vec();
        self.init_storage(slots.iter().map(|kv| (&kv.0, kv.1.as_ref())))
    }
}
