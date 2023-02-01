use crate::database::{
    metadata,
    Column,
    Database,
};
use fuel_core_relayer::ports::RelayerMetadata;
use fuel_core_storage::{
    StorageInspect,
    StorageMutate,
};

impl StorageInspect<RelayerMetadata> for Database {
    type Error = fuel_core_storage::Error;

    fn get(
        &self,
        _key: &<RelayerMetadata as fuel_core_storage::Mappable>::Key,
    ) -> Result<
        Option<
            std::borrow::Cow<
                <RelayerMetadata as fuel_core_storage::Mappable>::OwnedValue,
            >,
        >,
        Self::Error,
    > {
        Ok(Self::get(
            self,
            metadata::FINALIZED_DA_HEIGHT_KEY,
            Column::Metadata,
        )?)
    }

    fn contains_key(
        &self,
        _key: &<RelayerMetadata as fuel_core_storage::Mappable>::Key,
    ) -> Result<bool, Self::Error> {
        Ok(Self::exists(
            self,
            metadata::FINALIZED_DA_HEIGHT_KEY,
            Column::Metadata,
        )?)
    }
}

impl StorageMutate<RelayerMetadata> for Database {
    fn insert(
        &mut self,
        _key: &<RelayerMetadata as fuel_core_storage::Mappable>::Key,
        value: &<RelayerMetadata as fuel_core_storage::Mappable>::Value,
    ) -> Result<
        Option<<RelayerMetadata as fuel_core_storage::Mappable>::OwnedValue>,
        Self::Error,
    > {
        Ok(Self::insert(
            self,
            metadata::FINALIZED_DA_HEIGHT_KEY,
            Column::Metadata,
            value,
        )?)
    }

    fn remove(
        &mut self,
        _key: &<RelayerMetadata as fuel_core_storage::Mappable>::Key,
    ) -> Result<
        Option<<RelayerMetadata as fuel_core_storage::Mappable>::OwnedValue>,
        Self::Error,
    > {
        Ok(Self::remove(
            self,
            metadata::FINALIZED_DA_HEIGHT_KEY,
            Column::Metadata,
        )?)
    }
}
