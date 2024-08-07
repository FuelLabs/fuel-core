use async_graphql::{
    connection::CursorType,
    InputValueError,
    InputValueResult,
    Scalar,
    ScalarType,
    Value,
};
use fuel_core_types::{
    fuel_types,
    fuel_types::BlockHeight,
    tai64::Tai64,
};
use std::{
    array::TryFromSliceError,
    convert::TryInto,
    fmt::{
        Display,
        Formatter,
    },
    str::FromStr,
};
pub use tx_pointer::TxPointer;
pub use utxo_id::UtxoId;

pub mod message_id;
pub mod tx_pointer;
pub mod utxo_id;

macro_rules! number_scalar {
    ($i:ident, $t:ty, $name:expr) => {
        /// Need our own scalar type since GraphQL integers are restricted to i32.
        #[derive(
            Copy, Clone, Debug, derive_more::Into, derive_more::From, PartialEq, Eq,
        )]
        pub struct $i(pub $t);

        #[Scalar(name = $name)]
        impl ScalarType for $i {
            fn parse(value: Value) -> InputValueResult<Self> {
                if let Value::String(value) = &value {
                    let num: $t = value.parse().map_err(InputValueError::custom)?;
                    Ok($i(num))
                } else {
                    Err(InputValueError::expected_type(value))
                }
            }

            fn to_value(&self) -> Value {
                Value::String(self.0.to_string())
            }
        }

        impl Display for $i {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $i {
            type Err = core::num::ParseIntError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(<$t>::from_str(s)?))
            }
        }

        impl CursorType for $i {
            type Error = core::num::ParseIntError;

            fn decode_cursor(s: &str) -> Result<Self, Self::Error> {
                Self::from_str(s)
            }

            fn encode_cursor(&self) -> String {
                self.to_string()
            }
        }
    };
}

number_scalar!(U64, u64, "U64");
number_scalar!(U32, u32, "U32");
number_scalar!(U16, u16, "U16");
number_scalar!(U8, u8, "U8");

impl From<BlockHeight> for U32 {
    fn from(h: BlockHeight) -> Self {
        U32(*h)
    }
}

impl From<U32> for BlockHeight {
    fn from(u: U32) -> Self {
        u.0.into()
    }
}

impl From<U32> for usize {
    fn from(u: U32) -> Self {
        u.0 as usize
    }
}

/// Need our own u64 type since GraphQL integers are restricted to i32.
#[derive(Copy, Clone, Debug, derive_more::Into, derive_more::From)]
pub struct Tai64Timestamp(pub Tai64);

#[Scalar(name = "Tai64Timestamp")]
impl ScalarType for Tai64Timestamp {
    fn parse(value: Value) -> InputValueResult<Self> {
        if let Value::String(value) = &value {
            let num: u64 = value.parse().map_err(InputValueError::custom)?;
            Ok(Tai64Timestamp(Tai64(num)))
        } else {
            Err(InputValueError::expected_type(value))
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.0 .0.to_string())
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct SortedTxCursor {
    pub block_height: BlockHeight,
    pub tx_id: Bytes32,
}

impl SortedTxCursor {
    pub fn new(block_height: BlockHeight, tx_id: Bytes32) -> Self {
        Self {
            block_height,
            tx_id,
        }
    }
}

impl CursorType for SortedTxCursor {
    type Error = String;

    fn decode_cursor(s: &str) -> Result<Self, Self::Error> {
        let (block_height, tx_id) =
            s.split_once('#').ok_or("Incorrect format provided")?;

        Ok(Self::new(
            BlockHeight::from_str(block_height)
                .map_err(|_| "Failed to decode block_height")?,
            Bytes32::decode_cursor(tx_id)?,
        ))
    }

    fn encode_cursor(&self) -> String {
        format!("{}#{}", self.block_height, self.tx_id)
    }
}

#[derive(Clone, Debug, derive_more::Into, derive_more::From, PartialEq, Eq)]
pub struct HexString(pub(crate) Vec<u8>);

#[Scalar(name = "HexString")]
impl ScalarType for HexString {
    fn parse(value: Value) -> InputValueResult<Self> {
        if let Value::String(value) = &value {
            HexString::from_str(value.as_str()).map_err(Into::into)
        } else {
            Err(InputValueError::expected_type(value))
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.to_string())
    }
}

impl Display for HexString {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = format!("0x{}", hex::encode(&self.0));
        s.fmt(f)
    }
}

impl CursorType for HexString {
    type Error = String;

    fn decode_cursor(s: &str) -> Result<Self, Self::Error> {
        Self::from_str(s)
    }

    fn encode_cursor(&self) -> String {
        self.to_string()
    }
}

impl FromStr for HexString {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = s.strip_prefix("0x").unwrap_or(s);
        // decode into bytes
        let bytes = hex::decode(value).map_err(|e| e.to_string())?;
        Ok(HexString(bytes))
    }
}

impl From<fuel_types::Nonce> for HexString {
    fn from(n: fuel_types::Nonce) -> Self {
        HexString(n.to_vec())
    }
}

impl TryInto<fuel_types::Nonce> for HexString {
    type Error = TryFromSliceError;

    fn try_into(self) -> Result<fuel_types::Nonce, Self::Error> {
        let bytes: [u8; 32] = self.0.as_slice().try_into()?;
        Ok(fuel_types::Nonce::from(bytes))
    }
}

macro_rules! fuel_type_scalar {
    ($name:literal, $id:ident, $ft_id:ident, $len:expr) => {
        #[derive(Copy, Clone, Debug, PartialOrd, Ord, PartialEq, Eq)]
        pub struct $id(pub(crate) fuel_types::$ft_id);

        #[Scalar(name = $name)]
        impl ScalarType for $id {
            fn parse(value: Value) -> InputValueResult<Self> {
                if let Value::String(value) = &value {
                    $id::from_str(value.as_str()).map_err(Into::into)
                } else {
                    Err(InputValueError::expected_type(value))
                }
            }

            fn to_value(&self) -> Value {
                Value::String(self.to_string())
            }
        }

        impl FromStr for $id {
            type Err = String;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                // trim leading 0x
                let value = s.strip_prefix("0x").unwrap_or(s);
                // pad input to $len bytes
                let mut bytes = ((value.len() / 2)..$len).map(|_| 0).collect::<Vec<u8>>();
                // decode into bytes
                bytes.extend(hex::decode(value).map_err(|e| e.to_string())?);
                // attempt conversion to fixed length array, error if too long
                let bytes: [u8; $len] = bytes.try_into().map_err(|e: Vec<u8>| {
                    format!("expected {} bytes, received {}", $len, e.len())
                })?;

                Ok(Self(bytes.into()))
            }
        }

        impl Display for $id {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                let s = format!("0x{}", hex::encode(self.0));
                s.fmt(f)
            }
        }

        impl From<$id> for fuel_types::$ft_id {
            fn from(value: $id) -> Self {
                value.0
            }
        }

        impl From<fuel_types::$ft_id> for $id {
            fn from(value: fuel_types::$ft_id) -> Self {
                $id(value)
            }
        }

        impl CursorType for $id {
            type Error = String;

            fn decode_cursor(s: &str) -> Result<Self, Self::Error> {
                Self::from_str(s)
            }

            fn encode_cursor(&self) -> String {
                self.to_string()
            }
        }
    };
}

fuel_type_scalar!("Bytes32", Bytes32, Bytes32, 32);
fuel_type_scalar!("Address", Address, Address, 32);
fuel_type_scalar!("BlockId", BlockId, Bytes32, 32);
fuel_type_scalar!("AssetId", AssetId, AssetId, 32);
fuel_type_scalar!("BlobId", BlobId, BlobId, 32);
fuel_type_scalar!("ContractId", ContractId, ContractId, 32);
fuel_type_scalar!("Salt", Salt, Salt, 32);
fuel_type_scalar!("TransactionId", TransactionId, Bytes32, 32);
fuel_type_scalar!("RelayedTransactionId", RelayedTransactionId, Bytes32, 32);
fuel_type_scalar!("MessageId", MessageId, MessageId, 32);
fuel_type_scalar!("Nonce", Nonce, Nonce, 32);
fuel_type_scalar!("Signature", Signature, Bytes64, 64);

impl From<fuel_core_types::fuel_vm::Signature> for Signature {
    fn from(s: fuel_core_types::fuel_vm::Signature) -> Self {
        Self(<[u8; 64]>::from(s).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transaction_id_parseable_with_0x_prefix() {
        let id = "0x0101010101010101010101010101010101010101010101010101010101010101";
        let tx_id = TransactionId::from_str(id).expect("parseable with 0x");
        assert_eq!(*tx_id.0, [0x01; 32]);
    }

    #[test]
    fn transaction_id_parseable_without_0x_prefix() {
        let id = "0101010101010101010101010101010101010101010101010101010101010101";
        let tx_id = TransactionId::from_str(id).expect("parseable without 0x");
        assert_eq!(*tx_id.0, [0x01; 32]);
    }

    #[test]
    fn transaction_id_only_parses_valid_hex() {
        let id = "0xZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ";
        let res = TransactionId::from_str(id);
        assert!(res.is_err());
    }

    #[test]
    fn hex_string_parses_with_0x_prefix() {
        let hex_data = "0x0101";
        let parsed_data = HexString::from_str(hex_data).expect("parseable with 0x");
        assert_eq!(*parsed_data.0, [0x01; 2]);
    }

    #[test]
    fn hex_string_parses_without_0x_prefix() {
        let hex_data = "0101";
        let parsed_data = HexString::from_str(hex_data).expect("parseable without 0x");
        assert_eq!(*parsed_data.0, [0x01; 2]);
    }

    #[test]
    fn hex_string_only_parses_valid_hex() {
        let hex_data = "0xZZZZ";
        let res = HexString::from_str(hex_data);
        assert!(res.is_err());
    }
}
