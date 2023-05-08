use serde::{
    de::Error,
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
};
use std::{
    fmt::{
        Debug,
        Display,
        Formatter,
        LowerHex,
    },
    str::FromStr,
};

#[derive(Debug, thiserror::Error)]
pub enum HexFormatError {
    #[error("hex parsing error {0}")]
    HexError(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HexFormatted<T: Debug + Clone + Default + PartialEq>(pub T);

impl<T: LowerHex + Debug + Clone + Default + PartialEq> Serialize for HexFormatted<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(format!("{:#x}", self.0).as_str())
    }
}

impl<'de, T: FromStr<Err = E> + Debug + Clone + Default + PartialEq, E: Display>
    Deserialize<'de> for HexFormatted<T>
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        T::from_str(s.as_str()).map_err(D::Error::custom).map(Self)
    }
}

impl<T: FromStr<Err = E> + Debug + Clone + Default + PartialEq, E: Display> FromStr
    for HexFormatted<T>
{
    type Err = HexFormatError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        T::from_str(s)
            .map_err(|e| HexFormatError::HexError(format!("{e}")))
            .map(Self)
    }
}

impl<T: LowerHex + Debug + Clone + Default + PartialEq> Display for HexFormatted<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{:x}", self.0)
    }
}

impl<T: LowerHex + Debug + Clone + Default + PartialEq> LowerHex for HexFormatted<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{:x}", self.0)
    }
}

pub const fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'F' => Some(c - b'A' + 10),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'0'..=b'9' => Some(c - b'0'),
        _ => None,
    }
}
