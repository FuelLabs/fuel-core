use super::schema;
use crate::client::schema::{
    ConversionError,
    ConversionError::HexStringPrefixError,
};
use core::fmt;
use cynic::impl_scalar;
use fuel_core_types::{
    fuel_tx::PanicInstruction,
    fuel_types::BlockHeight,
};
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
    ops::Deref,
    str::FromStr,
};
use tai64::Tai64;

#[derive(Debug, Clone, Default)]
pub struct HexFormatted<T: Debug + Clone + Default>(pub T);

impl<T: LowerHex + Debug + Clone + Default> Serialize for HexFormatted<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(format!("{:#x}", self.0).as_str())
    }
}

impl<'de, T: FromStr<Err = E> + Debug + Clone + Default, E: Display> Deserialize<'de>
    for HexFormatted<T>
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        T::from_str(s.as_str()).map_err(D::Error::custom).map(Self)
    }
}

impl<T: FromStr<Err = E> + Debug + Clone + Default, E: Display> FromStr
    for HexFormatted<T>
{
    type Err = ConversionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        T::from_str(s)
            .map_err(|e| ConversionError::HexError(format!("{e}")))
            .map(Self)
    }
}

impl<T: LowerHex + Debug + Clone + Default> Display for HexFormatted<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{:x}", self.0)
    }
}

macro_rules! fuel_type_scalar {
    ($id:ident, $ft_id:ident) => {
        #[derive(cynic::Scalar, Debug, Clone, Default)]
        pub struct $id(pub HexFormatted<::fuel_core_types::fuel_types::$ft_id>);

        impl FromStr for $id {
            type Err = ConversionError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let b =
                    HexFormatted::<::fuel_core_types::fuel_types::$ft_id>::from_str(s)?;
                Ok($id(b))
            }
        }

        impl From<$id> for ::fuel_core_types::fuel_types::$ft_id {
            fn from(s: $id) -> Self {
                ::fuel_core_types::fuel_types::$ft_id::new(s.0 .0.into())
            }
        }

        impl From<::fuel_core_types::fuel_types::$ft_id> for $id {
            fn from(s: ::fuel_core_types::fuel_types::$ft_id) -> Self {
                $id(HexFormatted::<::fuel_core_types::fuel_types::$ft_id>(s))
            }
        }

        impl Display for $id {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                Display::fmt(&self.0, f)
            }
        }
    };
}

fuel_type_scalar!(Bytes32, Bytes32);
fuel_type_scalar!(Address, Address);
fuel_type_scalar!(BlockId, Bytes32);
fuel_type_scalar!(AssetId, AssetId);
fuel_type_scalar!(BlobId, BlobId);
fuel_type_scalar!(ContractId, ContractId);
fuel_type_scalar!(Salt, Salt);
fuel_type_scalar!(TransactionId, Bytes32);
fuel_type_scalar!(RelayedTransactionId, Bytes32);
fuel_type_scalar!(Signature, Bytes64);
fuel_type_scalar!(Nonce, Nonce);

impl LowerHex for Nonce {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        LowerHex::fmt(&self.0 .0, f)
    }
}

#[derive(cynic::Scalar, Debug, Clone, Default)]
pub struct UtxoId(pub HexFormatted<::fuel_core_types::fuel_tx::UtxoId>);

impl FromStr for UtxoId {
    type Err = ConversionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let b = HexFormatted::<::fuel_core_types::fuel_tx::UtxoId>::from_str(s)?;
        Ok(UtxoId(b))
    }
}

impl From<UtxoId> for ::fuel_core_types::fuel_tx::UtxoId {
    fn from(s: UtxoId) -> Self {
        s.0 .0
    }
}

impl From<fuel_core_types::fuel_tx::UtxoId> for UtxoId {
    fn from(value: fuel_core_types::fuel_tx::UtxoId) -> Self {
        Self(HexFormatted(value))
    }
}

impl LowerHex for UtxoId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        LowerHex::fmt(&self.0 .0, f)
    }
}

#[derive(cynic::Scalar, Debug, Clone, Default)]
pub struct TxPointer(pub HexFormatted<::fuel_core_types::fuel_tx::TxPointer>);

impl FromStr for TxPointer {
    type Err = ConversionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let b = HexFormatted::<::fuel_core_types::fuel_tx::TxPointer>::from_str(s)?;
        Ok(TxPointer(b))
    }
}

impl From<TxPointer> for ::fuel_core_types::fuel_tx::TxPointer {
    fn from(s: TxPointer) -> Self {
        s.0 .0
    }
}

impl LowerHex for TxPointer {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        LowerHex::fmt(&self.0 .0, f)
    }
}

#[derive(cynic::Scalar, Debug, Clone)]
pub struct HexString(pub Bytes);

impl From<HexString> for Vec<u8> {
    fn from(s: HexString) -> Self {
        s.0 .0
    }
}

impl Deref for HexString {
    type Target = Bytes;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct Bytes(pub Vec<u8>);

impl FromStr for Bytes {
    type Err = ConversionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // trim leading 0x
        let value = s.strip_prefix("0x").ok_or(HexStringPrefixError)?;
        // decode value into bytes
        Ok(Bytes(hex::decode(value)?))
    }
}

impl Serialize for Bytes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let hex = format!("0x{}", hex::encode(&self.0));
        serializer.serialize_str(hex.as_str())
    }
}

impl<'de> Deserialize<'de> for Bytes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        Self::from_str(s.as_str()).map_err(D::Error::custom)
    }
}

impl Display for Bytes {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{}", hex::encode(&self.0))
    }
}

impl Deref for Bytes {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

macro_rules! number_scalar {
    ($i:ident, $t:ty) => {
        #[derive(
            Debug, Clone, derive_more::Into, derive_more::From, PartialOrd, Eq, PartialEq,
        )]
        pub struct $i(pub $t);
        impl_scalar!($i, schema::$i);

        impl Serialize for $i {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                let s = self.0.to_string();
                serializer.serialize_str(s.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $i {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let s: String = Deserialize::deserialize(deserializer)?;
                Ok(Self(s.parse().map_err(D::Error::custom)?))
            }
        }
    };
}

number_scalar!(U64, u64);
number_scalar!(U32, u32);
number_scalar!(U16, u16);

impl TryFrom<U64> for PanicInstruction {
    type Error = ConversionError;

    fn try_from(s: U64) -> Result<Self, Self::Error> {
        Ok(s.0.into())
    }
}

impl From<usize> for U64 {
    fn from(i: usize) -> Self {
        U64(i as u64)
    }
}

impl From<U32> for BlockHeight {
    fn from(s: U32) -> Self {
        s.0.into()
    }
}

impl From<BlockHeight> for U32 {
    fn from(s: BlockHeight) -> U32 {
        (*s).into()
    }
}

#[derive(
    Debug, Clone, derive_more::Into, derive_more::From, PartialOrd, Eq, PartialEq,
)]
pub struct Tai64Timestamp(pub Tai64);
impl_scalar!(Tai64Timestamp, schema::Tai64Timestamp);

impl Tai64Timestamp {
    /// Convert Unix timestamp to `Tai64Timestamp`.
    pub fn from_unix(secs: i64) -> Self {
        Tai64Timestamp(Tai64::from_unix(secs))
    }

    /// Convert `Tai64Timestamp` to unix timestamp.
    pub fn to_unix(self) -> i64 {
        self.0.to_unix()
    }
}

impl Serialize for Tai64Timestamp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = self.0 .0.to_string();
        serializer.serialize_str(s.as_str())
    }
}

impl<'de> Deserialize<'de> for Tai64Timestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        Ok(Self(Tai64(s.parse().map_err(D::Error::custom)?)))
    }
}

impl BlockId {
    /// Converts the hash into a message having the same bytes.
    pub fn into_message(self) -> fuel_core_types::fuel_crypto::Message {
        let bytes: fuel_core_types::fuel_types::Bytes32 = self.into();
        fuel_core_types::fuel_crypto::Message::from_bytes(*bytes)
    }
}

impl Signature {
    pub fn into_signature(self) -> fuel_core_types::fuel_crypto::Signature {
        let bytes: fuel_core_types::fuel_types::Bytes64 = self.into();
        fuel_core_types::fuel_crypto::Signature::from_bytes(*bytes)
    }
}
