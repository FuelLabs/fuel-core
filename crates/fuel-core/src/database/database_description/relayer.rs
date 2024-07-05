use crate::database::database_description::DatabaseDescription;
use fuel_core_storage::kv_store::StorageColumn;
use fuel_core_types::blockchain::primitives::DaBlockHeight;

/// The column used by the relayer database in the case if the relayer is disabled.
#[derive(
    Debug,
    Copy,
    Clone,
    strum_macros::EnumCount,
    strum_macros::IntoStaticStr,
    PartialEq,
    Eq,
    enum_iterator::Sequence,
    Hash,
)]
pub enum DummyColumn {
    Metadata,
}

impl StorageColumn for DummyColumn {
    fn name(&self) -> String {
        let str: &str = self.into();
        str.to_string()
    }

    fn id(&self) -> u32 {
        *self as u32
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Relayer;

impl DatabaseDescription for Relayer {
    #[cfg(feature = "relayer")]
    type Column = fuel_core_relayer::storage::Column;

    #[cfg(not(feature = "relayer"))]
    type Column = DummyColumn;

    type Height = DaBlockHeight;

    fn version() -> u32 {
        0
    }

    fn name() -> String {
        "relayer".to_string()
    }

    fn metadata_column() -> Self::Column {
        Self::Column::Metadata
    }

    fn prefix(_: &Self::Column) -> Option<usize> {
        None
    }
}
