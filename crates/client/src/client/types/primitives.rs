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
    ($id:ident, $base_id:ident) => {
        #[derive(Debug, Clone, Default)]
        pub struct $id(pub HexFormatted<base::$base_id>);

        impl FromStr for $id {
            type Err = ConversionError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let b = HexFormatted::<base::$base_id>::from_str(s)?;
                Ok($id(b))
            }
        }

        impl From<$id> for base::$base_id {
            fn from(s: $id) -> Self {
                // s     -> $id
                // s.0   -> HexFormatted<$base_id>
                // s.0.0 -> $base_id
                s.0 .0
            }
        }

        impl From<base::$base_id> for $id {
            fn from(s: base::$base_id) -> Self {
                $id(HexFormatted::<base::$base_id>(s))
            }
        }

        impl Display for $id {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                Display::fmt(&self.0, f)
            }
        }
    };
}

pub mod base {
    use core::{
        fmt,
        str::FromStr,
    };

    impl<const N: usize, T> From<T> for Bytes<N>
    where
        T: Into<[u8; N]>,
    {
        fn from(value: T) -> Self {
            let b: [u8; N] = value.into();
            b.into()
        }
    }

    #[derive(Clone, Debug)]
    pub struct Bytes<const N: usize>(pub [u8; N]);

    impl<const N: usize> Bytes<N> {
        pub const fn new(bytes: [u8; N]) -> Self {
            Self(bytes)
        }
    }

    impl<const N: usize> Default for Bytes<N> {
        fn default() -> Self {
            Self([0; N])
        }
    }

    impl<const N: usize> AsRef<[u8]> for Bytes<N> {
        fn as_ref(&self) -> &[u8] {
            &self.0
        }
    }

    impl<const N: usize> AsMut<[u8]> for Bytes<N> {
        fn as_mut(&mut self) -> &mut [u8] {
            &mut self.0
        }
    }

    impl<const N: usize> fmt::Display for Bytes<N> {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            <Self as fmt::LowerHex>::fmt(&self, f)
        }
    }

    impl<const N: usize> fmt::LowerHex for Bytes<N> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            if f.alternate() {
                write!(f, "0x")?
            }

            match f.width() {
                Some(w) if w > 0 => self.0.chunks(2 * N / w).try_for_each(|c| {
                    write!(f, "{:02x}", c.iter().fold(0u8, |acc, x| acc ^ x))
                }),

                _ => self.0.iter().try_for_each(|b| write!(f, "{:02x}", &b)),
            }
        }
    }

    impl<const N: usize> FromStr for Bytes<N> {
        type Err = &'static str;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            const ERR: &str = "Invalid encoded byte";

            let alternate = s.starts_with("0x");

            let mut b = s.bytes();
            let mut ret = Self([0; N]);

            if alternate {
                b.next();
                b.next();
            }

            for r in ret.as_mut() {
                let h = b.next().and_then(hex_val).ok_or(ERR)?;
                let l = b.next().and_then(hex_val).ok_or(ERR)?;

                *r = h << 4 | l;
            }

            Ok(ret)
        }
    }

    pub type Bytes32 = Bytes<32>;
    pub type Bytes64 = Bytes<64>;

    const fn hex_val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'F' => Some(c - b'A' + 10),
            b'a'..=b'f' => Some(c - b'a' + 10),
            b'0'..=b'9' => Some(c - b'0'),
            _ => None,
        }
    }
}

fuel_type_scalar!(Address, Bytes32);
fuel_type_scalar!(AssetId, Bytes32);
fuel_type_scalar!(BlockId, Bytes32);
fuel_type_scalar!(ContractId, Bytes32);
fuel_type_scalar!(MerkleRoot, Bytes32);
fuel_type_scalar!(Message, Bytes32);
fuel_type_scalar!(MessageId, Bytes32);
fuel_type_scalar!(Nonce, Bytes32);
fuel_type_scalar!(PublicKey, Bytes64);
fuel_type_scalar!(Salt, Bytes32);
fuel_type_scalar!(Signature, Bytes64);
fuel_type_scalar!(TransactionId, Bytes32);

#[derive(Debug)]
pub struct Tai64Timestamp(pub Tai64);

impl From<Tai64> for Tai64Timestamp {
    fn from(value: Tai64) -> Self {
        Self(value)
    }
}

impl BlockId {
    pub fn into_message(self) -> Message {
        let bytes: base::Bytes32 = self.into();
        Message::from(bytes)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MyError {
    Error,
}

impl Display for MyError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl Signature {
    pub fn into_signature(self) -> Signature {
        let bytes: base::Bytes64 = self.into();
        Signature::from(bytes)
    }

    pub fn recover(self, message: &Message) -> Result<PublicKey, MyError> {
        let bytes: base::Bytes64 = self.into();
        let signature = fuel_core_types::fuel_crypto::Signature::from_bytes(bytes.0);
        let bytes: base::Bytes32 = (*message).clone().into();
        let message = fuel_core_types::fuel_crypto::Message::from_bytes(bytes.0);
        let bytes: base::Bytes64 = signature
            .recover(&message)
            .map_err(|_| MyError::Error)?
            .into();
        let public_key = PublicKey::from(bytes);
        Ok(public_key)
    }
}
