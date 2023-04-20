use crate::client::schema::ConversionError;
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
        #[derive(Debug, Clone, Default)]
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
fuel_type_scalar!(ContractId, ContractId);
fuel_type_scalar!(Salt, Salt);
fuel_type_scalar!(TransactionId, Bytes32);
fuel_type_scalar!(MessageId, MessageId);
fuel_type_scalar!(Signature, Bytes64);
fuel_type_scalar!(Nonce, Nonce);
fuel_type_scalar!(MerkleRoot, Bytes32);

pub struct Tai64Timestamp(pub Tai64);
