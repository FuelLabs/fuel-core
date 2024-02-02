use core::fmt::Debug;
use fuel_core_storage::kv_store::StorageColumn;

pub mod off_chain;
pub mod on_chain;
pub mod relayer;

/// The description of the database that makes it unique.
pub trait DatabaseDescription: 'static + Clone + Debug + Send + Sync {
    /// The type of the column used by the database.
    type Column: StorageColumn + strum::EnumCount + enum_iterator::Sequence;
    /// The type of the height of the database used to track commits.
    type Height: Copy;

    /// Returns the expected version of the database.
    fn version() -> u32;

    /// Returns the name of the database.
    fn name() -> &'static str;

    /// Returns the column used to store the metadata.
    fn metadata_column() -> Self::Column;

    /// Returns the prefix for the column.
    fn prefix(column: &Self::Column) -> Option<usize>;
}

/// The metadata of the database contains information about the version and its height.
#[derive(Copy, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum DatabaseMetadata<Height> {
    V1 { version: u32, height: Height },
}

impl<Height> DatabaseMetadata<Height> {
    /// Returns the version of the database.
    pub fn version(&self) -> u32 {
        match self {
            Self::V1 { version, .. } => *version,
        }
    }

    /// Returns the height of the database.
    pub fn height(&self) -> &Height {
        match self {
            Self::V1 { height, .. } => height,
        }
    }
}
