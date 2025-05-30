use crate::database::database_description::DatabaseDescription;
use fuel_core_storage::kv_store::StorageColumn;

pub const HISTORY_COLUMN_ID: u32 = u32::MAX / 2;
// Avoid conflicts with HistoricalDuplicateColumn indexes, which are
// in decreasing order starting from HISTORY_COLUMN_ID - 1.
pub const HISTORY_V2_COLUMN_ID: u32 = HISTORY_COLUMN_ID + 1;

#[derive(Debug, Copy, Clone, enum_iterator::Sequence)]
pub enum Column<Description>
where
    Description: DatabaseDescription,
{
    OriginalColumn(Description::Column),
    HistoricalDuplicateColumn(Description::Column),
    /// Original history column
    HistoryColumn,
    /// Migrated history column
    HistoryV2Column,
}

impl<Description> strum::EnumCount for Column<Description>
where
    Description: DatabaseDescription,
{
    const COUNT: usize = Description::Column::COUNT /* original columns */
        + Description::Column::COUNT /* duplicated columns */
        + 2 /* history column, history V2 column */;
}

impl<Description> StorageColumn for Column<Description>
where
    Description: DatabaseDescription,
{
    fn name(&self) -> String {
        match self {
            Column::OriginalColumn(c) => c.name(),
            Column::HistoricalDuplicateColumn(c) => {
                format!("history_{}", c.name())
            }
            Column::HistoryColumn => "modifications_history".to_string(),
            Column::HistoryV2Column => "modifications_history_v2".to_string(),
        }
    }

    fn id(&self) -> u32 {
        match self {
            Column::OriginalColumn(c) => c.id(),
            Column::HistoricalDuplicateColumn(c) => {
                historical_duplicate_column_id(c.id())
            }
            Column::HistoryColumn => HISTORY_COLUMN_ID,
            Column::HistoryV2Column => HISTORY_V2_COLUMN_ID,
        }
    }
}

pub fn historical_duplicate_column_id(id: u32) -> u32 {
    HISTORY_COLUMN_ID.saturating_sub(1).saturating_sub(id)
}

#[derive(Debug, Copy, Clone)]
pub struct Historical<Description> {
    _maker: core::marker::PhantomData<Description>,
}

impl<Description> DatabaseDescription for Historical<Description>
where
    Description: DatabaseDescription,
{
    type Column = Column<Description>;
    type Height = Description::Height;

    fn version() -> u32 {
        Description::version()
    }

    fn name() -> String {
        Description::name()
    }

    fn metadata_column() -> Self::Column {
        Column::OriginalColumn(Description::metadata_column())
    }

    fn prefix(column: &Self::Column) -> Option<usize> {
        match column {
            Column::OriginalColumn(c) => Description::prefix(c),
            Column::HistoricalDuplicateColumn(c) => {
                Some(Description::prefix(c).unwrap_or(0).saturating_add(8)) // `u64::to_be_bytes`
            }
            Column::HistoryColumn | Column::HistoryV2Column => Some(8),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::database_description::{
        DatabaseDescription,
        off_chain::OffChain,
        on_chain::OnChain,
        relayer::Relayer,
    };
    use strum::EnumCount;

    #[test]
    fn iteration_over_all_columns_on_chain() {
        let variants = enum_iterator::all::<Column<OnChain>>().collect::<Vec<_>>();
        let original = <OnChain as DatabaseDescription>::Column::COUNT;
        let duplicated = <OnChain as DatabaseDescription>::Column::COUNT;
        let history_modification_versions = 2;
        let expected_count = original + duplicated + history_modification_versions;
        assert_eq!(variants.len(), expected_count);
        assert_eq!(<Column<OnChain> as EnumCount>::COUNT, expected_count);
    }

    #[test]
    fn iteration_over_all_columns_off_chain() {
        let variants = enum_iterator::all::<Column<OffChain>>().collect::<Vec<_>>();
        let original = <OffChain as DatabaseDescription>::Column::COUNT;
        let duplicated = <OffChain as DatabaseDescription>::Column::COUNT;
        let history_modification_versions = 2;
        let expected_count = original + duplicated + history_modification_versions;
        assert_eq!(variants.len(), expected_count);
        assert_eq!(<Column<OffChain> as EnumCount>::COUNT, expected_count);
    }

    #[test]
    fn iteration_over_all_columns_relayer() {
        let variants = enum_iterator::all::<Column<Relayer>>().collect::<Vec<_>>();
        let original = <Relayer as DatabaseDescription>::Column::COUNT;
        let duplicated = <Relayer as DatabaseDescription>::Column::COUNT;
        let history_modification_versions = 2;
        let expected_count = original + duplicated + history_modification_versions;
        assert_eq!(variants.len(), expected_count);
        assert_eq!(<Column<Relayer> as EnumCount>::COUNT, expected_count);
    }
}
