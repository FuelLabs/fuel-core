use fuel_core_storage::{
    tables::FuelBlocks,
    transactional::Transaction,
    StorageInspect,
    StorageMutate,
};
use std::ops::DerefMut;

pub trait ExecutorDatabaseTrait<D> {
    type T: Transaction<D> + DerefMut<Target = D>;

    fn transaction(&self) -> Self::T;
}

pub trait StorageManipulation<D: StorageInspect<FuelBlocks> + StorageMutate<FuelBlocks>> {
    // type T: StorageMutate<FuelBlocks> + StorageInspect<FuelBlocks>;
    //
    fn insert(&self);
}
