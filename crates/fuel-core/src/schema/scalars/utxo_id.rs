use async_graphql::{
    connection::CursorType,
    InputValueError,
    InputValueResult,
    Scalar,
    ScalarType,
    Value,
};
use fuel_core_types::fuel_tx;
use std::{
    fmt::{
        Display,
        Formatter,
    },
    str::FromStr,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UtxoId(pub(crate) fuel_tx::UtxoId);

#[Scalar(name = "UtxoId")]
impl ScalarType for UtxoId {
    fn parse(value: Value) -> InputValueResult<Self> {
        if let Value::String(value) = &value {
            UtxoId::from_str(value.as_str()).map_err(Into::into)
        } else {
            Err(InputValueError::expected_type(value))
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.to_string())
    }
}

impl FromStr for UtxoId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        fuel_tx::UtxoId::from_str(s)
            .map(Self)
            .map_err(str::to_owned)
    }
}

impl Display for UtxoId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = format!("{:#x}", self.0);
        s.fmt(f)
    }
}

impl From<UtxoId> for fuel_tx::UtxoId {
    fn from(s: UtxoId) -> Self {
        s.0
    }
}

impl From<fuel_tx::UtxoId> for UtxoId {
    fn from(utxo_id: fuel_tx::UtxoId) -> Self {
        Self(utxo_id)
    }
}

impl CursorType for UtxoId {
    type Error = String;

    fn decode_cursor(s: &str) -> Result<Self, Self::Error> {
        Self::from_str(s)
    }

    fn encode_cursor(&self) -> String {
        self.to_string()
    }
}
