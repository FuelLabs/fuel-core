use crate::client::{
    types::primitives,
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
        pub struct $id(pub HexFormatted<primitives::$base_id>);

        impl FromStr for $id {
            type Err = HexFormatError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let b = HexFormatted::<primitives::$base_id>::from_str(s)?;
                Ok($id(b))
            }
        }

        impl From<$id> for primitives::$base_id {
            fn from(s: $id) -> Self {
                // s     -> $id
                // s.0   -> HexFormatted<$base_id>
                // s.0.0 -> $base_id
                s.0 .0
            }
        }

        impl From<primitives::$base_id> for $id {
            fn from(s: primitives::$base_id) -> Self {
                $id(HexFormatted::<primitives::$base_id>(s))
            }
        }

        impl Display for $id {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                Display::fmt(&self.0, f)
            }
        }
    };
}

client_type_scalar!(Address, Bytes32);
client_type_scalar!(AssetId, Bytes32);
client_type_scalar!(BlockId, Bytes32);
client_type_scalar!(ContractId, Bytes32);
client_type_scalar!(HexString, BytesN);
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
