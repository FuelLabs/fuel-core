use crate::{
    database::transaction::OwnedTransactionIndexCursor,
    model::BlockHeight,
};
use async_graphql::{
    connection::CursorType,
    InputValueError,
    InputValueResult,
    Scalar,
    ScalarType,
    Value,
};
use fuel_core_interfaces::common::fuel_types;
use std::{
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

/// Need our own u64 type since GraphQL integers are restricted to i32.
#[derive(Copy, Clone, Debug, derive_more::Into, derive_more::From)]
pub struct U64(pub u64);

#[Scalar(name = "U64")]
impl ScalarType for U64 {
    fn parse(value: Value) -> InputValueResult<Self> {
        if let Value::String(value) = &value {
            let num: u64 = value.parse().map_err(InputValueError::custom)?;
            Ok(U64(num))
        } else {
            Err(InputValueError::expected_type(value))
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.0.to_string())
    }
}

impl From<BlockHeight> for U64 {
    fn from(h: BlockHeight) -> Self {
        U64(h.to_usize() as u64)
    }
}

impl From<U64> for usize {
    fn from(u: U64) -> Self {
        u.0 as usize
    }
}

#[derive(Clone, Debug)]
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
            usize::from_str(block_height)
                .map_err(|_| "Failed to decode block_height")?
                .into(),
            Bytes32::decode_cursor(tx_id)?,
        ))
    }

    fn encode_cursor(&self) -> String {
        format!("{}#{}", self.block_height, self.tx_id)
    }
}

#[derive(Clone, Debug, derive_more::Into, derive_more::From)]
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

impl From<HexString> for OwnedTransactionIndexCursor {
    fn from(string: HexString) -> Self {
        string.0.into()
    }
}

impl From<OwnedTransactionIndexCursor> for HexString {
    fn from(cursor: OwnedTransactionIndexCursor) -> Self {
        HexString(cursor.into())
    }
}

impl FromStr for HexString {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = s.strip_prefix("0x").ok_or("expected 0x prefix")?;
        // decode into bytes
        let bytes = hex::decode(value).map_err(|e| e.to_string())?;
        Ok(HexString(bytes))
    }
}

macro_rules! fuel_type_scalar {
    ($name:literal, $id:ident, $ft_id:ident, $len:expr) => {
        #[derive(Copy, Clone, Debug)]
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
                let value = s.strip_prefix("0x").ok_or("expected 0x prefix")?;
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
fuel_type_scalar!("ContractId", ContractId, ContractId, 32);
fuel_type_scalar!("Salt", Salt, Salt, 32);
fuel_type_scalar!("TransactionId", TransactionId, Bytes32, 32);
fuel_type_scalar!("MessageId", MessageId, MessageId, 32);
fuel_type_scalar!("Signature", Signature, Bytes64, 64);

impl From<fuel_core_interfaces::common::prelude::Signature> for Signature {
    fn from(s: fuel_core_interfaces::common::prelude::Signature) -> Self {
        Self(<[u8; 64]>::from(s).into())
    }
}
