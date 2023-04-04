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
pub struct TxPointer(pub(crate) fuel_tx::TxPointer);

#[Scalar(name = "TxPointer")]
impl ScalarType for TxPointer {
    fn parse(value: Value) -> InputValueResult<Self> {
        if let Value::String(value) = &value {
            TxPointer::from_str(value.as_str()).map_err(Into::into)
        } else {
            Err(InputValueError::expected_type(value))
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.to_string())
    }
}

impl FromStr for TxPointer {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        fuel_tx::TxPointer::from_str(s)
            .map(Self)
            .map_err(str::to_owned)
    }
}

impl Display for TxPointer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = format!("{:#x}", self.0);
        s.fmt(f)
    }
}

impl From<TxPointer> for fuel_tx::TxPointer {
    fn from(s: TxPointer) -> Self {
        s.0
    }
}

impl From<fuel_tx::TxPointer> for TxPointer {
    fn from(tx_pointer: fuel_tx::TxPointer) -> Self {
        Self(tx_pointer)
    }
}

impl CursorType for TxPointer {
    type Error = String;

    fn decode_cursor(s: &str) -> Result<Self, Self::Error> {
        Self::from_str(s)
    }

    fn encode_cursor(&self) -> String {
        self.to_string()
    }
}
