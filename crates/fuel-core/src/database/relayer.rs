use crate::database::Column;
use fuel_core_relayer::ports::RelayerMetadata;

use super::storage::DatabaseColumn;

impl DatabaseColumn for RelayerMetadata {
    fn column() -> Column {
        Column::RelayerMetadata
    }
}
