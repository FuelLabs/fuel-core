use crate::database::{
    database_description::relayer::Relayer,
    Database,
};
use fuel_core_relayer::ports::Transactional;
use fuel_core_storage::transactional::StorageTransaction;

impl Transactional for Database<Relayer> {
    type Transaction<'a> = StorageTransaction<&'a mut Self> where Self: 'a;

    fn transaction(&mut self) -> Self::Transaction<'_> {
        StorageTransaction::new_transaction(self)
    }
}
