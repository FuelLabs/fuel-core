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

#[derive(Clone, Copy, Debug)]
pub struct MessageId(pub(crate) fuel_tx::MessageId);

#[Scalar(name = "MessageId")]
impl ScalarType for MessageId {
    fn parse(value: Value) -> InputValueResult<Self> {
        if let Value::String(value) = &value {
            MessageId::from_str(value.as_str()).map_err(Into::into)
        } else {
            Err(InputValueError::expected_type(value))
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.to_string())
    }
}

impl FromStr for MessageId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        fuel_tx::MessageId::from_str(s)
            .map(Self)
            .map_err(str::to_owned)
    }
}

impl Display for MessageId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = format!("{:#x}", self.0);
        s.fmt(f)
    }
}

impl From<MessageId> for fuel_tx::MessageId {
    fn from(s: MessageId) -> Self {
        s.0
    }
}

impl From<fuel_tx::MessageId> for MessageId {
    fn from(message_id: fuel_tx::MessageId) -> Self {
        Self(message_id)
    }
}

impl CursorType for MessageId {
    type Error = String;

    fn decode_cursor(s: &str) -> Result<Self, Self::Error> {
        Self::from_str(s)
    }

    fn encode_cursor(&self) -> String {
        self.to_string()
    }
}
