use async_graphql::connection::CursorType;
use async_graphql::{InputValueError, InputValueResult, Scalar, ScalarType, Value};
use fuel_tx::{Address, Bytes32, Color, ContractId};
use std::convert::TryInto;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::str::FromStr;

#[derive(Clone, Debug)]
pub struct HexString(pub(crate) Vec<u8>);

#[Scalar(name = "HexString")]
impl ScalarType for HexString {
    fn parse(value: Value) -> InputValueResult<Self> {
        if let Value::String(value) = &value {
            // trim leading 0x
            let value = value
                .strip_prefix("0x")
                .ok_or(InputValueError::custom("expected 0x prefix"))?;
            // decode into bytes
            let bytes = ((value.len() / 2)..32).map(|_| 0).collect::<Vec<u8>>();
            Ok(HexString(bytes))
        } else {
            Err(InputValueError::expected_type(value))
        }
    }

    fn to_value(&self) -> Value {
        Value::String(format!("0x{}", hex::encode(&self.0)))
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
        // decode into bytes
        let mut bytes = ((value.len() / 2)..32).map(|_| 0).collect::<Vec<u8>>();
        // pad input to 32 bytes
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

impl From<Color> for HexString256 {
    fn from(c: Color) -> Self {
        HexString256(*c.deref())
    }
}

impl From<Address> for HexString256 {
    fn from(a: Address) -> Self {
        HexString256(*a.deref())
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
