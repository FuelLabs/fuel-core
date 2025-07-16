use fuel_core_storage::column::Column;

pub(crate) struct ContractColumnsIterator {
    index: usize,
}

impl ContractColumnsIterator {
    pub fn new() -> Self {
        Self { index: 0 }
    }

    fn columns() -> &'static [Column] {
        static COLUMNS: [Column; 8] = [
            Column::ContractsRawCode,
            Column::ContractsState,
            Column::ContractsLatestUtxo,
            Column::ContractsAssets,
            Column::ContractsAssetsMerkleData,
            Column::ContractsAssetsMerkleMetadata,
            Column::ContractsStateMerkleData,
            Column::ContractsStateMerkleMetadata,
        ];
        &COLUMNS
    }
}

impl Iterator for ContractColumnsIterator {
    type Item = Column;

    fn next(&mut self) -> Option<Self::Item> {
        let columns = Self::columns();

        if self.index < columns.len() {
            let column = columns[self.index];
            self.index = self.index.saturating_add(1);
            Some(column)
        } else {
            None
        }
    }
}
