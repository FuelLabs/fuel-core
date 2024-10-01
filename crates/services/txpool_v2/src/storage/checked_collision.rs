use fuel_core_types::services::txpool::ArcPoolTx;
use std::collections::HashSet;

pub struct CheckedTransaction<StorageIndex> {
    tx: ArcPoolTx,
    direct_dependencies: HashSet<StorageIndex>,
    all_dependencies: HashSet<StorageIndex>,
}

impl<StorageIndex> CheckedTransaction<StorageIndex> {
    /// Creates a new checked transactions.
    ///
    /// It is a private method called by the `GraphStorage`.
    pub(super) fn new(
        tx: ArcPoolTx,
        direct_dependencies: HashSet<StorageIndex>,
        all_dependencies: HashSet<StorageIndex>,
    ) -> Self {
        Self {
            tx,
            direct_dependencies,
            all_dependencies,
        }
    }

    pub fn unpack(self) -> (ArcPoolTx, HashSet<StorageIndex>, HashSet<StorageIndex>) {
        (self.tx, self.direct_dependencies, self.all_dependencies)
    }
}

impl<StorageIndex> super::CheckedTransaction<StorageIndex>
    for CheckedTransaction<StorageIndex>
{
    fn tx(&self) -> &ArcPoolTx {
        &self.tx
    }

    fn into_tx(self) -> ArcPoolTx {
        self.tx
    }

    fn all_dependencies(&self) -> &HashSet<StorageIndex> {
        &self.all_dependencies
    }
}
