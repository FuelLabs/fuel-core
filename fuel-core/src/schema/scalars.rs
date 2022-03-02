use crate::database::transaction::OwnedTransactionIndexCursor;
use crate::model::fuel_block::BlockHeight;
use async_graphql::{
    connection::CursorType, InputValueError, InputValueResult, Scalar, ScalarType, Value,
};
use fuel_tx::{Address, AssetId, Bytes32, ContractId, Salt, UtxoId};
use std::{
    convert::TryInto,
    fmt::{Display, Formatter},
    ops::Deref,
    str::FromStr,
};

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
    pub tx_id: HexString256,
}

impl SortedTxCursor {
    pub fn new(block_height: BlockHeight, tx_id: HexString256) -> Self {
        Self {
            block_height,
            tx_id,
        }
    }
}

impl CursorType for SortedTxCursor {
    type Error = String;

    fn decode_cursor(s: &str) -> Result<Self, Self::Error> {
        let (block_height, tx_id) = s.split_once('#').ok_or("Incorrect format provided")?;

        Ok(Self::new(
            usize::from_str(block_height)
                .map_err(|_| "Failed to decode block_height")?
                .into(),
            HexString256::decode_cursor(tx_id)?,
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

#[derive(Copy, Clone, Debug)]
pub struct HexString256(pub(crate) [u8; 32]);

#[Scalar(name = "HexString256")]
impl ScalarType for HexString256 {
    fn parse(value: Value) -> InputValueResult<Self> {
        if let Value::String(value) = &value {
            HexString256::from_str(value.as_str()).map_err(Into::into)
        } else {
            Err(InputValueError::expected_type(value))
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.to_string())
    }
}

impl FromStr for HexString256 {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // trim leading 0x
        let value = s.strip_prefix("0x").ok_or("expected 0x prefix")?;
        // pad input to 32 bytes
        let mut bytes = ((value.len() / 2)..32).map(|_| 0).collect::<Vec<u8>>();
        // decode into bytes
        bytes.extend(hex::decode(value).map_err(|e| e.to_string())?);
        // attempt conversion to fixed length array, error if too long
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|e: Vec<u8>| format!("expected 32 bytes, received {}", e.len()))?;

        Ok(Self(bytes))
    }
}

impl Display for HexString256 {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = format!("0x{}", hex::encode(self.0));
        s.fmt(f)
    }
}

impl From<HexString256> for Bytes32 {
    fn from(s: HexString256) -> Self {
        Bytes32::new(s.0)
    }
}

impl From<HexString256> for Address {
    fn from(a: HexString256) -> Self {
        Address::new(a.0)
    }
}

impl From<Bytes32> for HexString256 {
    fn from(b: Bytes32) -> Self {
        HexString256(*b.deref())
    }
}

impl From<ContractId> for HexString256 {
    fn from(c: ContractId) -> Self {
        HexString256(*c.deref())
    }
}

impl From<AssetId> for HexString256 {
    fn from(c: AssetId) -> Self {
        HexString256(*c.deref())
    }
}

impl From<Address> for HexString256 {
    fn from(a: Address) -> Self {
        HexString256(*a.deref())
    }
}

impl From<Salt> for HexString256 {
    fn from(s: Salt) -> Self {
        HexString256(*s.deref())
    }
}

impl CursorType for HexString256 {
    type Error = String;

    fn decode_cursor(s: &str) -> Result<Self, Self::Error> {
        Self::from_str(s)
    }

    fn encode_cursor(&self) -> String {
        self.to_string()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct HexStringUtxoId(pub(crate) UtxoId);

#[Scalar(name = "HexStringUtxoId")]
impl ScalarType for HexStringUtxoId {
    fn parse(value: Value) -> InputValueResult<Self> {
        if let Value::String(value) = &value {
            HexStringUtxoId::from_str(value.as_str()).map_err(Into::into)
        } else {
            Err(InputValueError::expected_type(value))
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.to_string())
    }
}

impl FromStr for HexStringUtxoId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        UtxoId::from_str(s).map(Self).map_err(str::to_owned)
    }
}

impl Display for HexStringUtxoId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = format!("{:#x}", self.0);
        s.fmt(f)
    }
}

impl From<HexStringUtxoId> for UtxoId {
    fn from(s: HexStringUtxoId) -> Self {
        s.0
    }
}

impl From<UtxoId> for HexStringUtxoId {
    fn from(utxo_id: UtxoId) -> Self {
        Self(utxo_id)
    }
}

impl CursorType for HexStringUtxoId {
    type Error = String;

    fn decode_cursor(s: &str) -> Result<Self, Self::Error> {
        Self::from_str(s)
    }

    fn encode_cursor(&self) -> String {
        self.to_string()
    }
}
