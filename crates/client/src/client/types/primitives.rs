use crate::client::{
    HexFormatError,
    HexFormatted,
};

use std::{
    fmt::{
        Debug,
        Display,
        Formatter,
    },
    str::FromStr,
};
use tai64::Tai64;

macro_rules! client_type_scalar {
    ($id:ident, $base_id:ident) => {
        #[derive(Debug, Clone, Default)]
        pub struct $id(pub HexFormatted<base::$base_id>);

        impl FromStr for $id {
            type Err = HexFormatError;

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

client_type_scalar!(Address, Bytes32);
client_type_scalar!(AssetId, Bytes32);
client_type_scalar!(BlockId, Bytes32);
client_type_scalar!(ContractId, Bytes32);
client_type_scalar!(MerkleRoot, Bytes32);
client_type_scalar!(Message, Bytes32);
client_type_scalar!(MessageId, Bytes32);
client_type_scalar!(Nonce, Bytes32);
client_type_scalar!(PublicKey, Bytes64);
client_type_scalar!(Salt, Bytes32);
client_type_scalar!(Signature, Bytes64);
client_type_scalar!(TransactionId, Bytes32);

#[derive(Debug)]
pub struct Tai64Timestamp(pub Tai64);

impl From<Tai64> for Tai64Timestamp {
    fn from(value: Tai64) -> Self {
        Self(value)
    }
}
