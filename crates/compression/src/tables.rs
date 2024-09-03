use super::db::RocksDb;
use fuel_core_types::{
    fuel_compression::RawKey,
    fuel_tx::{
        Address,
        AssetId,
        ContractId,
    },
};
use std::collections::HashMap;

use rocksdb::WriteBatchWithTransaction;

/// Type-erased (serialized) data
#[derive(Debug, Clone)]
pub struct PostcardSerialized(Vec<u8>);
impl PostcardSerialized {
    pub(crate) fn new<T: serde::Serialize>(value: T) -> anyhow::Result<Self> {
        Ok(Self(postcard::to_stdvec(&value)?))
    }
}

macro_rules! check_keyspace {
    ($keyspace:expr, $expected:pat) => {
        match RegistryKeyspace::from_str($keyspace) {
            Some(val @ $expected) => val,
            Some(other) => {
                bail!("Keyspace {other:?} not valid for {}", stringify!($expected))
            }
            None => bail!("Unknown keyspace {:?}", $keyspace),
        }
    };
}
pub(crate) use check_keyspace;

macro_rules! tables {
    ($($name:ident: $type:ty),*$(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[allow(non_camel_case_types)] // Match names in structs exactly
        pub enum RegistryKeyspace {
            $(
                $name,
            )*
        }
        impl RegistryKeyspace {
            pub fn name(&self) -> &'static str {
                match self {
                    $(
                        Self::$name => stringify!($name),
                    )*
                }
            }
            pub fn from_str(name: &str) -> Option<Self> {
                match name {
                    $(
                        stringify!($name) => Some(Self::$name),
                    )*
                    _ => None,
                }
            }
        }


        #[derive(Debug, Clone, Default)]
        pub struct PerRegistryKeyspace<T> {
            $(pub $name: T,)*
        }
        impl<T> core::ops::Index<RegistryKeyspace> for PerRegistryKeyspace<T> {
            type Output = T;

            fn index(&self, index: RegistryKeyspace) -> &Self::Output {
                match index {
                    $(
                        RegistryKeyspace::$name => &self.$name,
                    )*
                }
            }
        }
        impl<T> core::ops::IndexMut<RegistryKeyspace> for PerRegistryKeyspace<T> {
            fn index_mut(&mut self, index: RegistryKeyspace) -> &mut Self::Output {
                match index {
                    $(
                        RegistryKeyspace::$name => &mut self.$name,
                    )*
                }
            }
        }

        #[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
        pub struct RegistrationsPerTable {
            $(pub $name: Vec<(RawKey, $type)>,)*
        }

        impl TryFrom<PerRegistryKeyspace<HashMap<RawKey, PostcardSerialized>>> for RegistrationsPerTable {
            type Error = anyhow::Error;

            fn try_from(value: PerRegistryKeyspace<HashMap<RawKey, PostcardSerialized>>) -> Result<Self, Self::Error> {
                let mut result = Self::default();
                $(
                    for (key, value) in value.$name.into_iter() {
                        result.$name.push((key, postcard::from_bytes(&value.0)?));
                    }
                )*
                Ok(result)
            }
        }

        impl RegistrationsPerTable {
            pub(crate) fn is_empty(&self) -> bool {
                $(
                    if !self.$name.is_empty() {
                        return false;
                    }
                )*
                true
            }

            pub(crate) fn write_all_tx(&self, db: &mut RocksDb) -> anyhow::Result<()> {
                let mut batch = WriteBatchWithTransaction::<false>::default();
                let cf_registry = db.db.cf_handle("temporal").unwrap();
                let cf_index = db.db.cf_handle("temporal_index").unwrap();

                $(
                    let mut key_table_prefix: Vec<u8> = stringify!($name).bytes().collect();
                    key_table_prefix.reserve(4);
                    key_table_prefix.push(0);

                    for (key, value) in self.$name.iter() {
                        // Get key bytes
                        let raw_key = postcard::to_stdvec(&key).expect("Never fails");

                        // Write new value
                        let db_key: Vec<u8> = key_table_prefix.iter().copied().chain(raw_key.clone()).collect();
                        let db_value = postcard::to_stdvec(&value).expect("Never fails");

                        println!("write_to_db {:?}", &db_key);

                        batch.put_cf(&cf_registry, db_key.clone(), db_value.clone());

                        // Remove the overwritten value from index, if any
                        if let Some(old_value) = db.db.get_cf(&cf_registry, db_key.clone())? {
                            let index_value: Vec<u8> = key_table_prefix.iter().copied().chain(old_value).collect();
                            batch.delete_cf(&cf_index, index_value);
                        }

                        // Add the new value to the index
                        let index_key: Vec<u8> = key_table_prefix.iter().copied().chain(db_value).collect();
                        batch.put_cf(&cf_index, index_key, raw_key);
                    }

                )*

                db.db.write(batch)?;
                Ok(())
            }
        }
    };
}

tables!(
    address: Address,
    asset_id: AssetId,
    contract_id: ContractId,
    script_code: Vec<u8>,
    witness: Vec<u8>,
);
