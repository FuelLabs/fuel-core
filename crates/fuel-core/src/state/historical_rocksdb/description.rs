use crate::database::database_description::DatabaseDescription;
use fuel_core_storage::kv_store::StorageColumn;

#[derive(Debug, Copy, Clone, enum_iterator::Sequence)]
pub enum Column<Description>
where
    Description: DatabaseDescription,
{
    UnderlyingDatabase(Description::Column),
    HistoryColumn,
}

impl<Description> strum::EnumCount for Column<Description>
where
    Description: DatabaseDescription,
{
    const COUNT: usize = Description::Column::COUNT + 1;
}

impl<Description> StorageColumn for Column<Description>
where
    Description: DatabaseDescription,
{
    fn name(&self) -> &'static str {
        match self {
            Column::UnderlyingDatabase(c) => c.name(),
            Column::HistoryColumn => "modifications_history",
        }
    }

    fn id(&self) -> u32 {
        match self {
            Column::UnderlyingDatabase(c) => c.id(),
            Column::HistoryColumn => u32::MAX,
        }
    }
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

    fn name() -> &'static str {
        Description::name()
    }

    fn metadata_column() -> Self::Column {
        Column::UnderlyingDatabase(Description::metadata_column())
    }

    fn prefix(column: &Self::Column) -> Option<usize> {
        let prefix = match column {
            Column::UnderlyingDatabase(c) => Description::prefix(c),
            Column::HistoryColumn => None,
        }
        .unwrap_or(0)
        .saturating_add(8); // `u64::to_be_bytes`

        Some(prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::database_description::{
        off_chain::OffChain,
        on_chain::OnChain,
        relayer::Relayer,
        DatabaseDescription,
    };
    use strum::EnumCount;

    #[test]
    fn iteration_over_all_columns_on_chain() {
        let variants = enum_iterator::all::<Column<OnChain>>().collect::<Vec<_>>();
        let expected_count = <OnChain as DatabaseDescription>::Column::COUNT + 1;
        assert_eq!(variants.len(), expected_count);
        assert_eq!(<Column<OnChain> as EnumCount>::COUNT, expected_count);
    }

    #[test]
    fn iteration_over_all_columns_off_chain() {
        let variants = enum_iterator::all::<Column<OffChain>>().collect::<Vec<_>>();
        let expected_count = <OffChain as DatabaseDescription>::Column::COUNT + 1;
        assert_eq!(variants.len(), expected_count);
        assert_eq!(<Column<OffChain> as EnumCount>::COUNT, expected_count);
    }

    #[test]
    fn iteration_over_all_columns_relayer() {
        let variants = enum_iterator::all::<Column<Relayer>>().collect::<Vec<_>>();
        let expected_count = <Relayer as DatabaseDescription>::Column::COUNT + 1;
        assert_eq!(variants.len(), expected_count);
        assert_eq!(<Column<Relayer> as EnumCount>::COUNT, expected_count);
    }
}
