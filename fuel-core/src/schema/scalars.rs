use async_graphql::{InputValueError, InputValueResult, Scalar, ScalarType, Value};
use std::convert::TryInto;

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
            // trim leading 0x
            let value = value
                .strip_prefix("0x")
                .ok_or(InputValueError::custom("expected 0x prefix"))?;
            // decode into bytes
            let mut bytes = ((value.len() / 2)..32).map(|_| 0).collect::<Vec<u8>>();
            // pad input to 32 bytes
            bytes.extend(hex::decode(value)?);
            // attempt conversion to fixed length array, error if too long
            let bytes: [u8; 32] = bytes
                .try_into()
                .map_err(|e: Vec<u8>| format!("expected 32 bytes, received {}", e.len()))?;

            Ok(HexString256(bytes))
        } else {
            Err(InputValueError::expected_type(value))
        }
    }

    fn to_value(&self) -> Value {
        Value::String(format!("0x{}", hex::encode(self.0)))
    }
}
