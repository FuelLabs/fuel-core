#![allow(unused)]

use fuel_core_storage::{
    ContractsAssetKey,
    ContractsStateKey,
    Error,
    Mappable,
    StorageBatchMutate,
    StorageInspect,
    StorageMutate,
    StorageRead,
    StorageSize,
    StorageWrite,
    blueprint::{
        BlueprintCodec,
        BlueprintInspect,
    },
    codec::{
        Decode,
        Encode,
        Encoder,
        primitive::Primitive,
        raw::Raw,
    },
    column::Column,
    merkle::sparse::KeyConverter,
    structured_storage::TableWithBlueprint,
    tables::{
        ConsensusParametersVersions,
        ContractsAssets,
        ContractsRawCode,
        ContractsState,
        FuelBlocks,
        StateTransitionBytecodeVersions,
        UploadedBytecodes,
        merkle::{
            ContractsAssetsMerkleData,
            ContractsAssetsMerkleMetadata,
        },
    },
};
use fuel_core_types::{
    fuel_tx::{
        AssetId,
        Bytes32,
        ContractId,
        Word,
    },
    fuel_types::bytes,
    fuel_vm::BlobData,
};
use parking_lot::Mutex;

#[cfg(feature = "std")]
use std::{
    borrow::Cow,
    borrow::ToOwned,
    collections::BTreeMap,
    sync::Arc,
};

#[cfg(not(feature = "std"))]
use alloc::{
    borrow::Cow,
    borrow::ToOwned,
    collections::BTreeMap,
    sync::Arc,
    vec::Vec,
};

/// Value state before and after execution
#[derive(Debug, Clone)]
pub(crate) struct ValueHistory<T> {
    /// First observed value
    pub first: Option<T>,
    /// Last observed value
    pub last: Option<T>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ContractAccesses {
    assets: BTreeMap<AssetId, ValueHistory<Word>>,
    slots: BTreeMap<Bytes32, ValueHistory<Vec<u8>>>,
}
impl ContractAccesses {
    pub fn assets_initial(&self) -> BTreeMap<AssetId, Word> {
        self.assets
            .iter()
            .map(|(k, v)| (*k, v.first.unwrap_or(0)))
            .collect()
    }

    pub fn assets_final(&self) -> BTreeMap<AssetId, Word> {
        self.assets
            .iter()
            .map(|(k, v)| (*k, v.last.unwrap_or(0)))
            .collect()
    }

    pub fn slots_initial(&self) -> BTreeMap<Bytes32, Option<&Vec<u8>>> {
        self.slots
            .iter()
            .map(|(k, v)| (*k, v.first.as_ref()))
            .collect()
    }

    pub fn slots_final(&self) -> BTreeMap<Bytes32, Option<&Vec<u8>>> {
        self.slots
            .iter()
            .map(|(k, v)| (*k, v.last.as_ref()))
            .collect()
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AccessedPerContract {
    pub per_contract: BTreeMap<ContractId, ContractAccesses>,
}

pub(crate) struct StorageAccessRecorder<S> {
    pub storage: S,
    pub record: Arc<Mutex<AccessedPerContract>>,
}

impl<S> StorageAccessRecorder<S> {
    pub fn new(storage: S) -> Self {
        Self {
            storage,
            record: Default::default(),
        }
    }

    fn read_asset(&self, key: &ContractsAssetKey, value: Option<Word>) {
        let contract_id = key.contract_id();
        let asset_id = key.asset_id();

        self.record
            .lock()
            .per_contract
            .entry(*contract_id)
            .or_default()
            .assets
            .entry(*asset_id)
            .or_insert(ValueHistory {
                first: value,
                last: value,
            });
    }

    fn write_asset(&self, key: &ContractsAssetKey, value: Word) {
        let contract_id = key.contract_id();
        let asset_id = key.asset_id();

        self.record
            .lock()
            .per_contract
            .entry(*contract_id)
            .or_default()
            .assets
            .entry(*asset_id)
            .and_modify(|e| e.last = Some(value));
    }

    fn remove_asset(&self, key: &ContractsAssetKey) {
        let contract_id = key.contract_id();
        let asset_id = key.asset_id();

        self.record
            .lock()
            .per_contract
            .entry(*contract_id)
            .or_default()
            .assets
            .entry(*asset_id)
            .and_modify(|e| e.last = None);
    }

    fn read_state(
        &self,
        key: &ContractsStateKey,
        value: Option<&<ContractsState as Mappable>::OwnedValue>,
    ) {
        let contract_id = key.contract_id();
        let state_key = key.state_key();

        self.record
            .lock()
            .per_contract
            .entry(*contract_id)
            .or_default()
            .slots
            .entry(*state_key)
            .or_insert(ValueHistory {
                first: value.map(|v| v.0.clone()),
                last: value.map(|v| v.0.clone()),
            });
    }

    fn write_state(
        &self,
        key: &ContractsStateKey,
        value: <ContractsState as Mappable>::OwnedValue,
    ) {
        let contract_id = key.contract_id();
        let state_key = key.state_key();

        self.record
            .lock()
            .per_contract
            .entry(*contract_id)
            .or_default()
            .slots
            .entry(*state_key)
            .and_modify(|e| e.last = Some(value.to_owned().into()));
    }

    fn remove_state(&self, key: &ContractsStateKey) {
        let contract_id = key.contract_id();
        let state_key = key.state_key();

        self.record
            .lock()
            .per_contract
            .entry(*contract_id)
            .or_default()
            .slots
            .entry(*state_key)
            .and_modify(|e| e.last = None);
    }
}

macro_rules! storage_passthrough {
    ($column:ident) => {
        impl<S> StorageInspect<$column> for StorageAccessRecorder<S>
        where
            S: StorageInspect<$column, Error = Error>,
        {
            type Error = S::Error;

            fn get(
                &self,
                key: &<$column as Mappable>::Key,
            ) -> Result<Option<Cow<<$column as Mappable>::OwnedValue>>, Self::Error> {
                <_ as StorageInspect<$column>>::get(&self.storage, key)
            }

            fn contains_key(
                &self,
                key: &<$column as Mappable>::Key,
            ) -> Result<bool, Self::Error> {
                <_ as StorageInspect<$column>>::contains_key(&self.storage, key)
            }
        }

        impl<S> StorageSize<$column> for StorageAccessRecorder<S>
        where
            S: StorageSize<$column> + StorageInspect<$column, Error = Error>,
        {
            fn size_of_value(
                &self,
                key: &<$column as Mappable>::Key,
            ) -> Result<Option<usize>, Error> {
                <_ as StorageSize<$column>>::size_of_value(&self.storage, key)
            }
        }

        impl<S> StorageMutate<$column> for StorageAccessRecorder<S>
        where
            S: StorageInspect<$column, Error = Error>
                + StorageMutate<$column, Error = Error>,
        {
            fn insert(
                &mut self,
                key: &<$column as Mappable>::Key,
                value: &<$column as Mappable>::Value,
            ) -> Result<(), Self::Error> {
                <_ as StorageMutate<$column>>::insert(&mut self.storage, key, value)
            }

            fn replace(
                &mut self,
                key: &<$column as Mappable>::Key,
                value: &<$column as Mappable>::Value,
            ) -> Result<Option<<$column as Mappable>::OwnedValue>, Self::Error> {
                <_ as StorageMutate<$column>>::replace(&mut self.storage, key, value)
            }

            fn remove(
                &mut self,
                key: &<$column as Mappable>::Key,
            ) -> Result<(), Self::Error> {
                <_ as StorageMutate<$column>>::remove(&mut self.storage, key)
            }

            fn take(
                &mut self,
                key: &<$column as Mappable>::Key,
            ) -> Result<Option<<$column as Mappable>::OwnedValue>, Self::Error> {
                <_ as StorageMutate<$column>>::take(&mut self.storage, key)
            }
        }

        impl<S> StorageRead<$column> for StorageAccessRecorder<S>
        where
            S: StorageRead<$column, Error = Error>,
            $column: Mappable + TableWithBlueprint<Column = Column>,
        {
            fn read(
                &self,
                key: &<$column as Mappable>::Key,
                offset: usize,
                buf: &mut [u8],
            ) -> Result<bool, Self::Error> {
                <_ as StorageRead<$column>>::read(&self.storage, key, offset, buf)
            }

            fn read_alloc(
                &self,
                key: &<$column as Mappable>::Key,
            ) -> Result<Option<Vec<u8>>, Self::Error> {
                <_ as StorageRead<$column>>::read_alloc(&self.storage, key)
            }
        }

        impl<S> StorageWrite<$column> for StorageAccessRecorder<S>
        where
            S: StorageWrite<$column, Error = Error>,
            $column: Mappable + TableWithBlueprint<Column = Column>,
        {
            fn write_bytes(
                &mut self,
                key: &<$column as Mappable>::Key,
                buf: &[u8],
            ) -> Result<(), Self::Error> {
                <_ as StorageWrite<$column>>::write_bytes(&mut self.storage, key, buf)
            }

            fn replace_bytes(
                &mut self,
                key: &<$column as Mappable>::Key,
                buf: &[u8],
            ) -> Result<Option<Vec<u8>>, Self::Error> {
                <_ as StorageWrite<$column>>::replace_bytes(&mut self.storage, key, buf)
            }

            fn take_bytes(
                &mut self,
                key: &<$column as Mappable>::Key,
            ) -> Result<Option<Vec<u8>>, Self::Error> {
                <_ as StorageWrite<$column>>::take_bytes(&mut self.storage, key)
            }
        }

        impl<S> StorageBatchMutate<$column> for StorageAccessRecorder<S>
        where
            S: StorageBatchMutate<$column, Error = Error>,
            $column: Mappable + TableWithBlueprint<Column = Column>,
        {
            fn init_storage<'a, Iter>(&mut self, set: Iter) -> Result<(), Error>
            where
                Iter: 'a
                    + Iterator<
                        Item = (
                            &'a <$column as Mappable>::Key,
                            &'a <$column as Mappable>::Value,
                        ),
                    >,
                <$column as Mappable>::Key: 'a,
                <$column as Mappable>::Value: 'a,
            {
                <_ as StorageBatchMutate<$column>>::init_storage(&mut self.storage, set)
            }

            fn insert_batch<'a, Iter>(&mut self, set: Iter) -> Result<(), Error>
            where
                Iter: 'a
                    + Iterator<
                        Item = (
                            &'a <$column as Mappable>::Key,
                            &'a <$column as Mappable>::Value,
                        ),
                    >,
                <$column as Mappable>::Key: 'a,
                <$column as Mappable>::Value: 'a,
            {
                <_ as StorageBatchMutate<$column>>::insert_batch(&mut self.storage, set)
            }

            fn remove_batch<'a, Iter>(&mut self, set: Iter) -> Result<(), Error>
            where
                Iter: 'a + Iterator<Item = &'a <$column as Mappable>::Key>,
                <$column as Mappable>::Key: 'a,
            {
                <_ as StorageBatchMutate<$column>>::remove_batch(&mut self.storage, set)
            }
        }
    };
}

storage_passthrough!(BlobData);
storage_passthrough!(ConsensusParametersVersions);
storage_passthrough!(ContractsRawCode);
storage_passthrough!(FuelBlocks);
storage_passthrough!(StateTransitionBytecodeVersions);
storage_passthrough!(UploadedBytecodes);

// Impl for ContractsAssets

impl<S> StorageInspect<ContractsAssets> for StorageAccessRecorder<S>
where
    S: StorageInspect<ContractsAssets, Error = Error>,
{
    type Error = S::Error;

    fn get(
        &self,
        key: &<ContractsAssets as Mappable>::Key,
    ) -> Result<Option<Cow<<ContractsAssets as Mappable>::OwnedValue>>, Self::Error> {
        let value = <_ as StorageInspect<ContractsAssets>>::get(&self.storage, key)?;
        self.read_asset(key, value.as_ref().map(|v| v.clone().into_owned()));
        Ok(value)
    }

    fn contains_key(
        &self,
        key: &<ContractsAssets as Mappable>::Key,
    ) -> Result<bool, Self::Error> {
        <_ as StorageInspect<ContractsAssets>>::contains_key(&self.storage, key)
    }
}

impl<S> StorageSize<ContractsAssets> for StorageAccessRecorder<S>
where
    S: StorageSize<ContractsAssets> + StorageInspect<ContractsAssets, Error = Error>,
{
    fn size_of_value(
        &self,
        key: &<ContractsAssets as Mappable>::Key,
    ) -> Result<Option<usize>, Error> {
        <_ as StorageSize<ContractsAssets>>::size_of_value(&self.storage, key)
    }
}

impl<S> StorageMutate<ContractsAssets> for StorageAccessRecorder<S>
where
    S: StorageInspect<ContractsAssets, Error = Error>
        + StorageMutate<ContractsAssets, Error = Error>,
{
    fn insert(
        &mut self,
        key: &<ContractsAssets as Mappable>::Key,
        value: &<ContractsAssets as Mappable>::Value,
    ) -> Result<(), Self::Error> {
        <_ as StorageMutate<ContractsAssets>>::insert(&mut self.storage, key, value)?;
        self.write_asset(key, *value);
        Ok(())
    }

    fn replace(
        &mut self,
        key: &<ContractsAssets as Mappable>::Key,
        value: &<ContractsAssets as Mappable>::Value,
    ) -> Result<Option<<ContractsAssets as Mappable>::OwnedValue>, Self::Error> {
        let old_value = <_ as StorageMutate<ContractsAssets>>::replace(
            &mut self.storage,
            key,
            value,
        )?;
        self.read_asset(key, old_value);
        self.write_asset(key, *value);
        Ok(old_value)
    }

    fn remove(
        &mut self,
        key: &<ContractsAssets as Mappable>::Key,
    ) -> Result<(), Self::Error> {
        <_ as StorageMutate<ContractsAssets>>::remove(&mut self.storage, key)?;
        self.remove_asset(key);
        Ok(())
    }

    fn take(
        &mut self,
        key: &<ContractsAssets as Mappable>::Key,
    ) -> Result<Option<<ContractsAssets as Mappable>::OwnedValue>, Self::Error> {
        let value = <_ as StorageMutate<ContractsAssets>>::take(&mut self.storage, key)?;
        self.read_asset(key, value);
        self.remove_asset(key);
        Ok(value)
    }
}

impl<S> StorageRead<ContractsAssets> for StorageAccessRecorder<S>
where
    S: StorageRead<ContractsAssets, Error = Error>,
    ContractsAssets: Mappable + TableWithBlueprint<Column = Column>,
{
    fn read(
        &self,
        key: &<ContractsAssets as Mappable>::Key,
        offset: usize,
        buf: &mut [u8],
    ) -> Result<bool, Self::Error> {
        // We need to get the whole value to record the access
        let value = <S as StorageRead<ContractsAssets>>::read_alloc(&self.storage, key)?;

        let value_dec = value
            .as_ref()
            .map(|v| {
                <<ContractsAssets as TableWithBlueprint>::Blueprint as BlueprintCodec<
                    ContractsAssets,
                >>::ValueCodec::decode(v)
            })
            .transpose()?;

        self.read_asset(key, value_dec);

        let Some(bytes) = value else {
            return Ok(false);
        };

        let bytes_len = bytes.len();
        let start = offset;
        let end = offset.saturating_add(buf.len());

        if end > bytes_len {
            return Err(Error::Other(anyhow::anyhow!(
                "Read out of bounds: value length is {bytes_len}, read range is {start}..{end}"
            )));
        }

        let starting_from_offset = &bytes[start..end];
        buf[..].copy_from_slice(starting_from_offset);
        Ok(true)
    }

    fn read_alloc(
        &self,
        key: &<ContractsAssets as Mappable>::Key,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        let value = <S as StorageRead<ContractsAssets>>::read_alloc(&self.storage, key)?;

        let value_dec = value
            .as_ref()
            .map(|v| {
                <<ContractsAssets as TableWithBlueprint>::Blueprint as BlueprintCodec<
                    ContractsAssets,
                >>::ValueCodec::decode(v)
            })
            .transpose()?;

        self.read_asset(key, value_dec);
        Ok(value)
    }
}

impl<S> StorageWrite<ContractsAssets> for StorageAccessRecorder<S>
where
    S: StorageWrite<ContractsAssets, Error = Error>,
    ContractsAssets: Mappable + TableWithBlueprint<Column = Column>,
{
    fn write_bytes(
        &mut self,
        key: &<ContractsAssets as Mappable>::Key,
        buf: &[u8],
    ) -> Result<(), Self::Error> {
        let new_value =
            <<ContractsAssets as TableWithBlueprint>::Blueprint as BlueprintCodec<
                ContractsAssets,
            >>::ValueCodec::decode(buf)?;

        <_ as StorageWrite<ContractsAssets>>::write_bytes(&mut self.storage, key, buf)?;
        self.write_asset(key, new_value);
        Ok(())
    }

    fn replace_bytes(
        &mut self,
        key: &<ContractsAssets as Mappable>::Key,
        buf: &[u8],
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        let new_value =
            <<ContractsAssets as TableWithBlueprint>::Blueprint as BlueprintCodec<
                ContractsAssets,
            >>::ValueCodec::decode(buf)?;

        let old_value = <_ as StorageWrite<ContractsAssets>>::replace_bytes(
            &mut self.storage,
            key,
            buf,
        )?;
        let old_value_dec = old_value
            .as_ref()
            .map(|v| {
                <<ContractsAssets as TableWithBlueprint>::Blueprint as BlueprintCodec<
                    ContractsAssets,
                >>::ValueCodec::decode(v)
            })
            .transpose()?;

        self.read_asset(key, old_value_dec);
        self.write_asset(key, new_value);

        Ok(old_value)
    }

    fn take_bytes(
        &mut self,
        key: &<ContractsAssets as Mappable>::Key,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        let old_value =
            <_ as StorageWrite<ContractsAssets>>::take_bytes(&mut self.storage, key)?;

        let old_value_dec = old_value
            .as_ref()
            .map(|v| {
                <<ContractsAssets as TableWithBlueprint>::Blueprint as BlueprintCodec<
                    ContractsAssets,
                >>::ValueCodec::decode(v)
            })
            .transpose()?;

        self.read_asset(key, old_value_dec);
        self.remove_asset(key);

        Ok(old_value)
    }
}

impl<S> StorageBatchMutate<ContractsAssets> for StorageAccessRecorder<S>
where
    S: StorageBatchMutate<ContractsAssets, Error = Error>,
    ContractsAssets: Mappable + TableWithBlueprint<Column = Column>,
{
    fn init_storage<'a, Iter>(&mut self, set: Iter) -> Result<(), Error>
    where
        Iter: 'a
            + Iterator<
                Item = (
                    &'a <ContractsAssets as Mappable>::Key,
                    &'a <ContractsAssets as Mappable>::Value,
                ),
            >,
        <ContractsAssets as Mappable>::Key: 'a,
        <ContractsAssets as Mappable>::Value: 'a,
    {
        let mut items = Vec::new();
        for (k, v) in set {
            self.write_asset(k, *v);
            items.push((k, v));
        }
        <_ as StorageBatchMutate<ContractsAssets>>::init_storage(
            &mut self.storage,
            items.into_iter(),
        )
    }

    fn insert_batch<'a, Iter>(&mut self, set: Iter) -> Result<(), Error>
    where
        Iter: 'a
            + Iterator<
                Item = (
                    &'a <ContractsAssets as Mappable>::Key,
                    &'a <ContractsAssets as Mappable>::Value,
                ),
            >,
        <ContractsAssets as Mappable>::Key: 'a,
        <ContractsAssets as Mappable>::Value: 'a,
    {
        let mut items = Vec::new();
        for (k, v) in set {
            self.write_asset(k, *v);
            items.push((k, v));
        }
        <_ as StorageBatchMutate<ContractsAssets>>::insert_batch(
            &mut self.storage,
            items.into_iter(),
        )
    }

    fn remove_batch<'a, Iter>(&mut self, set: Iter) -> Result<(), Error>
    where
        Iter: 'a + Iterator<Item = &'a <ContractsAssets as Mappable>::Key>,
        <ContractsAssets as Mappable>::Key: 'a,
    {
        let mut items = Vec::new();
        for k in set {
            self.remove_asset(k);
            items.push(k);
        }
        <_ as StorageBatchMutate<ContractsAssets>>::remove_batch(
            &mut self.storage,
            items.into_iter(),
        )
    }
}

// Impl for ContractsState
impl<S> StorageInspect<ContractsState> for StorageAccessRecorder<S>
where
    S: StorageInspect<ContractsState, Error = Error>,
{
    type Error = S::Error;

    fn get(
        &self,
        key: &<ContractsState as Mappable>::Key,
    ) -> Result<Option<Cow<<ContractsState as Mappable>::OwnedValue>>, Self::Error> {
        let value = <_ as StorageInspect<ContractsState>>::get(&self.storage, key)?;
        self.read_state(key, value.as_deref());
        Ok(value)
    }

    fn contains_key(
        &self,
        key: &<ContractsState as Mappable>::Key,
    ) -> Result<bool, Self::Error> {
        <_ as StorageInspect<ContractsState>>::contains_key(&self.storage, key)
    }
}

impl<S> StorageSize<ContractsState> for StorageAccessRecorder<S>
where
    S: StorageSize<ContractsState> + StorageInspect<ContractsState, Error = Error>,
{
    fn size_of_value(
        &self,
        key: &<ContractsState as Mappable>::Key,
    ) -> Result<Option<usize>, Error> {
        <_ as StorageSize<ContractsState>>::size_of_value(&self.storage, key)
    }
}

impl<S> StorageMutate<ContractsState> for StorageAccessRecorder<S>
where
    S: StorageInspect<ContractsState, Error = Error>
        + StorageMutate<ContractsState, Error = Error>,
{
    fn insert(
        &mut self,
        key: &<ContractsState as Mappable>::Key,
        value: &<ContractsState as Mappable>::Value,
    ) -> Result<(), Self::Error> {
        <_ as StorageMutate<ContractsState>>::insert(&mut self.storage, key, value)?;
        self.write_state(key, (*value).into());
        Ok(())
    }

    fn replace(
        &mut self,
        key: &<ContractsState as Mappable>::Key,
        value: &<ContractsState as Mappable>::Value,
    ) -> Result<Option<<ContractsState as Mappable>::OwnedValue>, Self::Error> {
        let old_value =
            <_ as StorageMutate<ContractsState>>::replace(&mut self.storage, key, value)?;
        self.read_state(key, old_value.as_ref());
        self.write_state(key, value.into());
        Ok(old_value)
    }

    fn remove(
        &mut self,
        key: &<ContractsState as Mappable>::Key,
    ) -> Result<(), Self::Error> {
        <_ as StorageMutate<ContractsState>>::remove(&mut self.storage, key)?;
        self.remove_state(key);
        Ok(())
    }

    fn take(
        &mut self,
        key: &<ContractsState as Mappable>::Key,
    ) -> Result<Option<<ContractsState as Mappable>::OwnedValue>, Self::Error> {
        let value = <_ as StorageMutate<ContractsState>>::take(&mut self.storage, key)?;
        self.read_state(key, value.as_ref());
        self.remove_state(key);
        Ok(value)
    }
}

impl<S> StorageRead<ContractsState> for StorageAccessRecorder<S>
where
    S: StorageRead<ContractsState, Error = Error>,
    ContractsState: Mappable + TableWithBlueprint<Column = Column>,
{
    fn read(
        &self,
        key: &<ContractsState as Mappable>::Key,
        offset: usize,
        buf: &mut [u8],
    ) -> Result<bool, Self::Error> {
        // We need to get the whole value to record the access
        let value = <S as StorageRead<ContractsState>>::read_alloc(&self.storage, key)?;

        let value_dec = value
            .as_ref()
            .map(|v| {
                <<ContractsState as TableWithBlueprint>::Blueprint as BlueprintCodec<
                    ContractsState,
                >>::ValueCodec::decode(v)
            })
            .transpose()?;

        self.read_state(key, value_dec.as_ref());

        let Some(bytes) = value else {
            return Ok(false);
        };

        let bytes_len = bytes.len();
        let start = offset;
        let end = offset.saturating_add(buf.len());

        if end > bytes_len {
            return Err(Error::Other(anyhow::anyhow!(
                "Read out of bounds: value length is {bytes_len}, read range is {start}..{end}"
            )));
        }

        let starting_from_offset = &bytes[start..end];
        buf[..].copy_from_slice(starting_from_offset);
        Ok(true)
    }

    fn read_alloc(
        &self,
        key: &<ContractsState as Mappable>::Key,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        let value = <S as StorageRead<ContractsState>>::read_alloc(&self.storage, key)?;

        let value_dec = value
            .as_ref()
            .map(|v| {
                <<ContractsState as TableWithBlueprint>::Blueprint as BlueprintCodec<
                    ContractsState,
                >>::ValueCodec::decode(v)
            })
            .transpose()?;

        self.read_state(key, value_dec.as_ref());

        Ok(value)
    }
}

impl<S> StorageWrite<ContractsState> for StorageAccessRecorder<S>
where
    S: StorageWrite<ContractsState, Error = Error>,
    ContractsState: Mappable + TableWithBlueprint<Column = Column>,
{
    fn write_bytes(
        &mut self,
        key: &<ContractsState as Mappable>::Key,
        buf: &[u8],
    ) -> Result<(), Self::Error> {
        let new_value =
            <<ContractsState as TableWithBlueprint>::Blueprint as BlueprintCodec<
                ContractsState,
            >>::ValueCodec::decode(buf)?;

        <_ as StorageWrite<ContractsState>>::write_bytes(&mut self.storage, key, buf)?;
        self.write_state(key, new_value);
        Ok(())
    }

    fn replace_bytes(
        &mut self,
        key: &<ContractsState as Mappable>::Key,
        buf: &[u8],
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        let new_value =
            <<ContractsState as TableWithBlueprint>::Blueprint as BlueprintCodec<
                ContractsState,
            >>::ValueCodec::decode(buf)?;

        let old_value = <_ as StorageWrite<ContractsState>>::replace_bytes(
            &mut self.storage,
            key,
            buf,
        )?;
        let old_value_dec = old_value
            .as_ref()
            .map(|v| {
                <<ContractsState as TableWithBlueprint>::Blueprint as BlueprintCodec<
                    ContractsState,
                >>::ValueCodec::decode(v)
            })
            .transpose()?;

        self.read_state(key, old_value_dec.as_ref());
        self.write_state(key, new_value);

        Ok(old_value)
    }

    fn take_bytes(
        &mut self,
        key: &<ContractsState as Mappable>::Key,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        let old_value =
            <_ as StorageWrite<ContractsState>>::take_bytes(&mut self.storage, key)?;

        let old_value_dec = old_value
            .as_ref()
            .map(|v| {
                <<ContractsState as TableWithBlueprint>::Blueprint as BlueprintCodec<
                    ContractsState,
                >>::ValueCodec::decode(v)
            })
            .transpose()?;

        self.read_state(key, old_value_dec.as_ref());
        self.remove_state(key);

        Ok(old_value)
    }
}

impl<S> StorageBatchMutate<ContractsState> for StorageAccessRecorder<S>
where
    S: StorageBatchMutate<ContractsState, Error = Error>,
    ContractsState: Mappable + TableWithBlueprint<Column = Column>,
{
    fn init_storage<'a, Iter>(&mut self, set: Iter) -> Result<(), Error>
    where
        Iter: 'a
            + Iterator<
                Item = (
                    &'a <ContractsState as Mappable>::Key,
                    &'a <ContractsState as Mappable>::Value,
                ),
            >,
        <ContractsState as Mappable>::Key: 'a,
        <ContractsState as Mappable>::Value: 'a,
    {
        let mut items = Vec::new();
        for (k, v) in set {
            self.write_state(k, (*v).into());
            items.push((k, v));
        }
        <_ as StorageBatchMutate<ContractsState>>::init_storage(
            &mut self.storage,
            items.into_iter(),
        )
    }

    fn insert_batch<'a, Iter>(&mut self, set: Iter) -> Result<(), Error>
    where
        Iter: 'a
            + Iterator<
                Item = (
                    &'a <ContractsState as Mappable>::Key,
                    &'a <ContractsState as Mappable>::Value,
                ),
            >,
        <ContractsState as Mappable>::Key: 'a,
        <ContractsState as Mappable>::Value: 'a,
    {
        let mut items = Vec::new();
        for (k, v) in set {
            self.write_state(k, (*v).into());
            items.push((k, v));
        }
        <_ as StorageBatchMutate<ContractsState>>::insert_batch(
            &mut self.storage,
            items.into_iter(),
        )
    }

    fn remove_batch<'a, Iter>(&mut self, set: Iter) -> Result<(), Error>
    where
        Iter: 'a + Iterator<Item = &'a <ContractsState as Mappable>::Key>,
        <ContractsState as Mappable>::Key: 'a,
    {
        let mut items = Vec::new();
        for k in set {
            self.remove_state(k);
            items.push(k);
        }
        <_ as StorageBatchMutate<ContractsState>>::remove_batch(
            &mut self.storage,
            items.into_iter(),
        )
    }
}
