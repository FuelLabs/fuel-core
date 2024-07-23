use core::fmt;

use serde::{
    ser::SerializeTuple,
    Deserialize,
    Serialize,
};

use fuel_core_types::fuel_compression::{
    access,
    tables,
    Key,
    KeyPerTable,
    Table,
};

/// New registrations written to a specific table.
#[derive(Clone, PartialEq, Eq)]
pub struct WriteTo<T: Table> {
    /// The values are inserted starting from this key
    pub start_key: Key<T>,
    /// Values, inserted using incrementing ids starting from `start_key`
    pub values: Vec<T::Type>,
}

impl<T: Table> fmt::Debug for WriteTo<T>
where
    T::Type: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.values.is_empty() {
            return f.write_str("WriteTo::EMPTY");
        }

        f.debug_struct("WriteTo")
            .field("start_key", &self.start_key)
            .field("values", &self.values)
            .finish()
    }
}

impl<T: Table> WriteTo<T>
where
    T::Type: PartialEq,
{
    /// Reverse lookup.
    /// TODO: possibly add a lookup table for this, if deemed necessary
    pub fn lookup_value(&self, needle: &T::Type) -> Option<Key<T>> {
        if *needle == T::Type::default() {
            return Some(Key::DEFAULT_VALUE);
        }

        let mut key = self.start_key;
        for v in &self.values {
            if v == needle {
                return Some(key);
            }
            key = key.next();
        }
        None
    }
}

/// Custom serialization is used to omit the start_key when the sequence is empty
impl<T> Serialize for WriteTo<T>
where
    T: Table + Serialize,
{
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut tup = serializer.serialize_tuple(2)?;
        tup.serialize_element(&self.values)?;
        if self.values.is_empty() {
            tup.serialize_element(&())?;
        } else {
            tup.serialize_element(&self.start_key)?;
        }
        tup.end()
    }
}

impl<'de, T: Table> Deserialize<'de> for WriteTo<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_tuple(
            2,
            Self {
                start_key: Key::ZERO,
                values: Vec::new(),
            },
        )
    }
}

impl<'de, T: Table + Deserialize<'de>> serde::de::Visitor<'de> for WriteTo<T> {
    type Value = WriteTo<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(concat!("WriteTo<", stringify!(T), "> instance"))
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let values: Vec<T::Type> = seq.next_element()?.ok_or(
            serde::de::Error::invalid_length(0, &"WriteTo<_> with 2 elements"),
        )?;

        if values.is_empty() {
            seq.next_element()?.ok_or(serde::de::Error::invalid_length(
                1,
                &"WriteTo<_> with 2 elements",
            ))?;
            Ok(WriteTo {
                start_key: Key::ZERO,
                values,
            })
        } else {
            let start_key: Key<T> = seq.next_element()?.ok_or(
                serde::de::Error::invalid_length(1, &"WriteTo<_> with 2 elements"),
            )?;
            Ok(WriteTo { start_key, values })
        }
    }
}

/// Registeration section of the compressed block
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registrations {
    /// Merkle root of the registeration table merkle roots
    pub tables_root: [u8; 32],
    /// Changes per table
    pub changes: ChangesPerTable,
}

macro_rules! tables {
    ($($name:ident),*$(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[allow(non_snake_case)] // Match table/type names exactly
        pub struct ChangesPerTable {
            $(pub $name: WriteTo<tables::$name>,)*
        }

        impl ChangesPerTable {
            pub fn from_start_keys(start_keys: KeyPerTable) -> Self {
                Self {
                    $($name: WriteTo {
                        start_key: start_keys.$name,
                        values: Vec::new(),
                    }),*
                }
            }

            pub fn is_empty(&self) -> bool {
                $(self.$name.values.is_empty() &&)* true
            }


            /// Apply changes to the db
            pub fn write_to_db(&self, reg: &mut crate::db::RocksDb) -> anyhow::Result<()> {
                $(
                    reg.batch_write(self.$name.start_key, self.$name.values.clone())?;
                )*
                Ok(())
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

tables!(AssetId, Address, ContractId, ScriptCode, Witness);

#[cfg(test)]
mod tests {
    use super::*;
    use bincode::Options;
    use fuel_core_types::{
        fuel_asm::op,
        fuel_tx::{
            AssetId,
            ContractId,
        },
        fuel_types::Address,
    };

    #[test]
    fn test_tables() {
        let original = Registrations {
            tables_root: Default::default(),
            changes: ChangesPerTable {
                AssetId: WriteTo {
                    start_key: Key::try_from(100).unwrap(),
                    values: vec![*AssetId::from([0xa0; 32]), *AssetId::from([0xa1; 32])],
                },
                Address: WriteTo {
                    start_key: Key::ZERO,
                    values: vec![*Address::from([0xb0; 32])],
                },
                ContractId: WriteTo {
                    start_key: Key::ZERO,
                    values: vec![*ContractId::from([0xc0; 32])],
                },
                ScriptCode: WriteTo {
                    start_key: Key::ZERO,
                    values: vec![
                        vec![op::addi(0x20, 0x20, 1), op::ret(0)]
                            .into_iter()
                            .collect(),
                        vec![op::muli(0x20, 0x20, 5), op::ret(1)]
                            .into_iter()
                            .collect(),
                    ],
                },
                Witness: WriteTo {
                    start_key: Key::ZERO,
                    values: vec![],
                },
            },
        };

        let pc_compressed = postcard::to_stdvec(&original).unwrap();
        let pc_decompressed: Registrations =
            postcard::from_bytes(&pc_compressed).unwrap();
        assert_eq!(original, pc_decompressed);

        let bc_opt = bincode::DefaultOptions::new().with_varint_encoding();

        let bc_compressed = bc_opt.serialize(&original).unwrap();
        let bc_decompressed: Registrations = bc_opt.deserialize(&bc_compressed).unwrap();
        assert_eq!(original, bc_decompressed);

        println!("data: {original:?}");
        println!("postcard compressed size {}", pc_compressed.len());
        println!("bincode  compressed size {}", bc_compressed.len());
        println!("postcard compressed: {:x?}", pc_compressed);
        println!("bincode  compressed: {:x?}", bc_compressed);

        // panic!("ok, just showing the results");
    }
}
