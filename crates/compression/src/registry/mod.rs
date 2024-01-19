use serde::{
    Deserialize,
    Serialize,
};

mod block_section;
pub(crate) mod db;
pub(crate) mod in_memory;
mod key;

use self::block_section::WriteTo;
pub use self::key::Key;

pub(crate) use self::block_section::Registrations;

mod _private {
    pub trait Seal {}
}

pub trait Table: _private::Seal {
    const NAME: &'static str;
    type Type: Default + Serialize + for<'de> Deserialize<'de>;
}

macro_rules! tables {
    // $index muse use increasing numbers starting from zero
    ($($name:ident: $ty:ty),*$(,)?) => { paste::paste!{
        pub mod tables {
            $(
                /// Specifies the table to use for a given key.
                /// The data is separated to tables based on the data type being stored.
                #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
                pub struct $name;

                impl super::_private::Seal for $name {}
                impl super::Table for $name {
                    const NAME: &'static str = stringify!($name);
                    type Type = $ty;
                }

                // Type level magic
                pub trait [<TypeLevel $name>]: super::_private::Seal {}
                impl [<TypeLevel $name>] for $name {}
            )*
        }

        /// One counter per table
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
        #[allow(non_snake_case)] // The field names match table type names eactly
        pub struct CountPerTable {
            $(pub $name: usize),*
        }

        impl CountPerTable {
            pub fn by_table<T: Table>(&self) -> usize {
                match T::NAME {
                    $(
                        stringify!($name) => self.$name,
                    )*
                    _ => unreachable!(),
                }
            }
        }

        /// One key value per table
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
        #[allow(non_snake_case)] // The field names match table type names eactly
        pub struct KeyPerTable {
            $(pub $name: Key<tables::$name>),*
        }

        impl KeyPerTable {
            pub fn by_table<T: Table>(&self) -> Key<T> {
                match T::NAME {
                    $(
                        stringify!($name) => Key::<T>::from_raw(self.$name.raw()),
                    )*
                    _ => unreachable!(),
                }
            }

            pub fn mut_by_table<T: Table>(&self) -> Key<T> {
                match T::NAME {
                    $(
                        stringify!($name) => Key::<T>::from_raw(self.$name.raw()),
                    )*
                    _ => unreachable!(),
                }
            }
        }

        pub fn next_keys<R: db::RegistrySelectNextKey>(reg: &mut R) -> KeyPerTable {
            KeyPerTable {
                $( $name: reg.next_key(), )*
            }
        }

        /// Registeration changes per table
        #[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
        #[allow(non_snake_case)] // The field names match table type names eactly
        pub struct ChangesPerTable {
            $(pub $name: WriteTo<tables::$name>),*
        }

        impl ChangesPerTable {
            pub fn push<T: Table>(&self, value: T::Type) -> &mut WriteTo<T> {
                match T::NAME {
                    $(
                        stringify!($name) => self.$name.values.push(value),
                    )*
                    _ => unreachable!(),
                }
            }

            pub fn apply(&self, reg: &mut impl db::RegistryWrite) {
                $(
                    reg.batch_write(self.$name.start_key, self.$name.values.clone());
                )*
            }
        }
    }};
}

tables!(
    AssetId: [u8; 32],
    Address: [u8; 32],
    ScriptCode: Vec<u8>,
    Witness: Vec<u8>,
);

#[cfg(test)]
mod tests {
    use fuel_core_types::fuel_types::AssetId;
    use tests::key::RawKey;

    use super::*;

    use super::db::{
        RegistryIndex as _,
        RegistryRead as _,
        RegistryWrite as _,
    };

    #[test]
    fn test_in_memory_db() {
        let mut reg = in_memory::InMemoryRegistry::default();

        // Empty
        assert_eq!(
            reg.read(Key::<tables::AssetId>::try_from(100).unwrap()),
            *AssetId::default()
        );
        assert_eq!(
            reg.index_lookup(&*AssetId::from([1; 32])),
            None::<Key<tables::AssetId>>
        );

        // Write
        reg.batch_write(
            Key::<tables::AssetId>::from_raw(RawKey::try_from(100u32).unwrap()),
            vec![*AssetId::from([1; 32]), *AssetId::from([2; 32])],
        );
        assert_eq!(
            reg.read(Key::<tables::AssetId>::try_from(100).unwrap()),
            *AssetId::from([1; 32])
        );
        assert_eq!(
            reg.read(Key::<tables::AssetId>::try_from(101).unwrap()),
            *AssetId::from([2; 32])
        );
        assert_eq!(
            reg.read(Key::<tables::AssetId>::try_from(102).unwrap()),
            *AssetId::default()
        );

        // Overwrite
        reg.batch_write(
            Key::<tables::AssetId>::from_raw(RawKey::try_from(99u32).unwrap()),
            vec![*AssetId::from([10; 32]), *AssetId::from([11; 32])],
        );
        assert_eq!(
            reg.read(Key::<tables::AssetId>::try_from(99).unwrap()),
            *AssetId::from([10; 32])
        );
        assert_eq!(
            reg.read(Key::<tables::AssetId>::try_from(100).unwrap()),
            *AssetId::from([11; 32])
        );

        // Wrapping
        reg.batch_write(
            Key::<tables::AssetId>::from_raw(RawKey::MAX),
            vec![*AssetId::from([3; 32]), *AssetId::from([4; 32])],
        );

        assert_eq!(
            reg.read(Key::<tables::AssetId>::from_raw(RawKey::MAX)),
            *AssetId::from([3; 32])
        );

        assert_eq!(
            reg.read(Key::<tables::AssetId>::from_raw(RawKey::MIN)),
            *AssetId::from([4; 32])
        );

        assert_eq!(
            reg.index_lookup(&*AssetId::from([3; 32])),
            Some(Key::<tables::AssetId>::from_raw(RawKey::MAX))
        );

        assert_eq!(
            reg.index_lookup(&*AssetId::from([4; 32])),
            Some(Key::<tables::AssetId>::from_raw(RawKey::MIN))
        );
    }
}
