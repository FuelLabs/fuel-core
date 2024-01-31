use serde::{
    Deserialize,
    Serialize,
};

pub(crate) mod block_section;
pub mod db;
pub(crate) mod in_memory;
mod key;

use self::block_section::WriteTo;
pub use self::{
    db::RegistryDb,
    key::Key,
};

mod _private {
    pub trait Seal {}
}

pub trait Table: _private::Seal {
    const NAME: &'static str;
    type Type: PartialEq + Default + Serialize + for<'de> Deserialize<'de>;
}

pub mod access {
    pub trait AccessCopy<T, V: Copy> {
        fn value(&self) -> V;
    }

    pub trait AccessRef<T, V> {
        fn get(&self) -> &V;
    }

    pub trait AccessMut<T, V> {
        fn get_mut(&mut self) -> &mut V;
    }
}

macro_rules! tables {
    // $index muse use increasing numbers starting from zero
    ($($name:ident: $ty:ty),*$(,)?) => {
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
            )*
        }

        /// One counter per table
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
        #[allow(non_snake_case)] // The field names match table type names eactly
        #[non_exhaustive]
        pub struct CountPerTable {
            $(pub $name: usize),*
        }

        impl CountPerTable {$(
            /// Custom constructor per table
            #[allow(non_snake_case)] // The field names match table type names eactly
            pub fn $name(value: usize) -> Self {
                Self {
                    $name: value,
                    ..Self::default()
                }
            }
        )*}

        $(
            impl access::AccessCopy<tables::$name, usize> for CountPerTable {
                fn value(&self) -> usize {
                    self.$name
                }
            }
        )*

        impl core::ops::Add<CountPerTable> for CountPerTable {
            type Output = Self;

            fn add(self, rhs: CountPerTable) -> Self::Output {
                Self {
                    $($name: self.$name + rhs.$name),*
                }
            }
        }

        impl core::ops::AddAssign<CountPerTable> for CountPerTable {
            fn add_assign(&mut self, rhs: CountPerTable) {
                $(self.$name += rhs.$name);*
            }
        }

        /// One key value per table
        #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        #[allow(non_snake_case)] // The field names match table type names eactly
        #[non_exhaustive]
        pub struct KeyPerTable {
            $(pub $name: Key<tables::$name>),*
        }

        impl Default for KeyPerTable {
            fn default() -> Self {
                Self {
                    $($name: Key::ZERO,)*
                }
            }
        }

        $(
            impl access::AccessCopy<tables::$name, Key<tables::$name>> for KeyPerTable {
                fn value(&self) -> Key<tables::$name> {
                    self.$name
                }
            }
            impl access::AccessRef<tables::$name, Key<tables::$name>> for KeyPerTable {
                fn get(&self) -> &Key<tables::$name> {
                    &self.$name
                }
            }
            impl access::AccessMut<tables::$name, Key<tables::$name>> for KeyPerTable {
                fn get_mut(&mut self) -> &mut Key<tables::$name> {
                    &mut self.$name
                }
            }
        )*

        pub fn next_keys<R: RegistryDb>(reg: &mut R) -> KeyPerTable {
            KeyPerTable {
                $( $name: reg.next_key(), )*
            }
        }

        /// Used to add together keys and counts to deterimine possible overwrite range
        pub fn add_keys(keys: KeyPerTable, counts: CountPerTable) -> KeyPerTable {
            KeyPerTable {
                $(
                    $name: keys.$name.add_u32(counts.$name.try_into()
                        .expect("Count too large. Shoudn't happen as we control inputs here.")
                    ),
                )*
            }
        }

        /// Registeration changes per table
        #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        #[allow(non_snake_case)] // The field names match table type names eactly
        #[non_exhaustive]
        pub struct ChangesPerTable {
            $(pub $name: WriteTo<tables::$name>),*
        }

        impl ChangesPerTable {
            pub fn is_empty(&self) -> bool {
                true $(&& self.$name.values.is_empty())*
            }

            pub fn from_start_keys(start_keys: KeyPerTable) -> Self {
                Self {
                    $($name: WriteTo {
                        start_key: start_keys.$name,
                        values: Vec::new(),
                    }),*
                }
            }

            /// Apply changes to the registry db
            pub fn apply_to_registry<R: RegistryDb>(&self, reg: &mut R) {
                $(
                    reg.batch_write(self.$name.start_key, self.$name.values.clone());
                )*
            }
        }

        $(
            impl access::AccessRef<tables::$name, WriteTo<tables::$name>> for ChangesPerTable {
                fn get(&self) -> &WriteTo<tables::$name> {
                    &self.$name
                }
            }
            impl access::AccessMut<tables::$name, WriteTo<tables::$name>> for ChangesPerTable {
                fn get_mut(&mut self) -> &mut WriteTo<tables::$name> {
                    &mut self.$name
                }
            }
        )*
    };
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

    #[test]
    fn test_in_memory_db() {
        let mut reg = in_memory::InMemoryRegistry::default();

        // Empty
        assert_eq!(
            reg.read(Key::<tables::AssetId>::try_from(100).unwrap()),
            [0; 32]
        );
        assert_eq!(
            reg.index_lookup(&*AssetId::from([1; 32])),
            None::<Key<tables::AssetId>>
        );

        // Write
        reg.batch_write(
            Key::<tables::AssetId>::from_raw(RawKey::try_from(100u32).unwrap()),
            vec![[1; 32], [2; 32]],
        );
        assert_eq!(
            reg.read(Key::<tables::AssetId>::try_from(100).unwrap()),
            [1; 32]
        );
        assert_eq!(
            reg.read(Key::<tables::AssetId>::try_from(101).unwrap()),
            [2; 32]
        );
        assert_eq!(
            reg.read(Key::<tables::AssetId>::try_from(102).unwrap()),
            [0; 32]
        );

        // Overwrite
        reg.batch_write(
            Key::<tables::AssetId>::from_raw(RawKey::try_from(99u32).unwrap()),
            vec![[10; 32], [11; 32]],
        );
        assert_eq!(
            reg.read(Key::<tables::AssetId>::try_from(99).unwrap()),
            [10; 32]
        );
        assert_eq!(
            reg.read(Key::<tables::AssetId>::try_from(100).unwrap()),
            [11; 32]
        );

        // Wrapping
        reg.batch_write(
            Key::<tables::AssetId>::from_raw(RawKey::MAX_WRITABLE),
            vec![[3; 32], [4; 32]],
        );

        assert_eq!(
            reg.read(Key::<tables::AssetId>::from_raw(RawKey::MAX_WRITABLE)),
            [3; 32]
        );

        assert_eq!(
            reg.read(Key::<tables::AssetId>::from_raw(RawKey::ZERO)),
            [4; 32]
        );

        assert_eq!(
            reg.index_lookup(&*AssetId::from([3; 32])),
            Some(Key::<tables::AssetId>::from_raw(RawKey::MAX_WRITABLE))
        );

        assert_eq!(
            reg.index_lookup(&*AssetId::from([4; 32])),
            Some(Key::<tables::AssetId>::from_raw(RawKey::ZERO))
        );
    }
}
