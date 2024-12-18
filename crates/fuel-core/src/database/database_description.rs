use core::fmt::Debug;
use fuel_core_storage::kv_store::StorageColumn;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_types::BlockHeight,
};
use std::collections::HashSet;
use strum::IntoEnumIterator;

pub mod gas_price;
pub mod off_chain;
pub mod on_chain;
pub mod relayer;

pub trait DatabaseHeight: PartialEq + Default + Debug + Copy + Send + Sync {
    fn as_u64(&self) -> u64;

    fn advance_height(&self) -> Option<Self>;

    fn rollback_height(&self) -> Option<Self>;
}

impl DatabaseHeight for BlockHeight {
    fn as_u64(&self) -> u64 {
        let height: u32 = (*self).into();
        height as u64
    }

    fn advance_height(&self) -> Option<Self> {
        self.succ()
    }

    fn rollback_height(&self) -> Option<Self> {
        self.pred()
    }
}

impl DatabaseHeight for DaBlockHeight {
    fn as_u64(&self) -> u64 {
        self.0
    }

    fn advance_height(&self) -> Option<Self> {
        self.0.checked_add(1).map(Into::into)
    }

    fn rollback_height(&self) -> Option<Self> {
        self.0.checked_sub(1).map(Into::into)
    }
}

/// The description of the database that makes it unique.
pub trait DatabaseDescription: 'static + Copy + Debug + Send + Sync {
    /// The type of the column used by the database.
    type Column: StorageColumn + strum::EnumCount + enum_iterator::Sequence;
    /// The type of the height of the database used to track commits.
    type Height: DatabaseHeight;

    /// Returns the expected version of the database.
    fn version() -> u32;

    /// Returns the name of the database.
    fn name() -> String;

    /// Returns the column used to store the metadata.
    fn metadata_column() -> Self::Column;

    /// Returns the prefix for the column.
    fn prefix(column: &Self::Column) -> Option<usize>;
}

#[derive(
    Copy,
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    Eq,
    PartialEq,
    Hash,
    strum::EnumIter,
)]
pub enum IndexationKind {
    Balances,
    CoinsToSpend,
}

impl IndexationKind {
    pub fn all() -> impl Iterator<Item = Self> {
        Self::iter()
    }
}

/// The metadata of the database contains information about the version and its height.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum DatabaseMetadata<Height> {
    V1 {
        version: u32,
        height: Height,
    },
    V2 {
        version: u32,
        height: Height,
        indexation_availability: HashSet<IndexationKind>,
    },
}

impl<Height> DatabaseMetadata<Height> {
    /// Returns the version of the database.
    pub fn version(&self) -> u32 {
        match self {
            Self::V1 { version, .. } => *version,
            Self::V2 { version, .. } => *version,
        }
    }

    /// Returns the height of the database.
    pub fn height(&self) -> &Height {
        match self {
            Self::V1 { height, .. } => height,
            Self::V2 { height, .. } => height,
        }
    }

    /// Returns true if the given indexation kind is available.
    pub fn indexation_available(&self, kind: IndexationKind) -> bool {
        match self {
            Self::V1 { .. } => false,
            Self::V2 {
                indexation_availability,
                ..
            } => indexation_availability.contains(&kind),
        }
    }
}

/// Gets the indexation availability from the metadata.
pub fn indexation_availability<D>(
    metadata: Option<DatabaseMetadata<D::Height>>,
) -> HashSet<IndexationKind>
where
    D: DatabaseDescription,
{
    match metadata {
        Some(DatabaseMetadata::V1 { .. }) => HashSet::new(),
        Some(DatabaseMetadata::V2 {
            indexation_availability,
            ..
        }) => indexation_availability.clone(),
        // If the metadata doesn't exist, it is a new database,
        // and we should set all indexation kinds to available.
        None => IndexationKind::all().collect(),
    }
}
