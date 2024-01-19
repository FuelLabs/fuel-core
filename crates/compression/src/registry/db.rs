use super::{
    Key,
    Table,
};

pub trait RegistrySelectNextKey {
    fn next_key<T: Table>(&mut self) -> Key<T>;
}

pub trait RegistryRead {
    fn read<T: Table>(&self, key: Key<T>) -> T::Type;
}

pub trait RegistryWrite {
    fn batch_write<T: Table>(&mut self, start_key: Key<T>, values: Vec<T::Type>);
}

pub trait RegistryIndex {
    fn index_lookup<T: Table>(&self, value: &T::Type) -> Option<Key<T>>;
}
