use fuel_core_storage::{
    transactional::Transaction,
};
use std::ops::DerefMut;

pub trait ExecutorDatabaseTrait<D> {
    type T: Transaction<D> + DerefMut<Target = D>;

    fn transaction(&self) -> Self::T;
}