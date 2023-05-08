use crate::client::{
    types::primitives,
    HexFormatError,
    HexFormatted,
};

use crate::client::types::primitives::Bytes32;
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

pub trait ClientScalar {
    type PrimitiveType;
}

macro_rules! client_type_scalar {
    ($id:ident, $base_id:ident) => {
        #[derive(Debug, Clone, Default, PartialEq)]
        pub struct $id(pub HexFormatted<primitives::$base_id>);

        impl $id {
            pub const fn new(
                raw: <primitives::$base_id as primitives::Primitive>::Raw,
            ) -> Self {
                let primitive = primitives::$base_id::new(raw);
                Self(HexFormatted::<_>(primitive))
            }
        }

        impl AsRef<[u8]> for $id {
            fn as_ref(&self) -> &[u8] {
                // s     -> $id
                // s.0   -> HexFormatted<$base_id>
                // s.0.0 -> $base_id
                self.0 .0.as_ref()
            }
        }

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
            fn from(primitive: primitives::$base_id) -> Self {
                $id(HexFormatted::<primitives::$base_id>(primitive))
            }
        }

        impl Display for $id {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                Display::fmt(&self.0, f)
            }
        }

        impl LowerHex for $id {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let val = &self.0;
                std::fmt::LowerHex::fmt(val, f)
            }
        }

        impl ClientScalar for $id {
            type PrimitiveType = primitives::$base_id;
        }
    };
}

client_type_scalar!(Address, Bytes32);
client_type_scalar!(AssetId, Bytes32);
client_type_scalar!(BlockId, Bytes32);
client_type_scalar!(ContractId, Bytes32);
client_type_scalar!(Hash, Bytes32);
client_type_scalar!(HexString, BytesN);
client_type_scalar!(MerkleRoot, Bytes32);
client_type_scalar!(MessageId, Bytes32);
client_type_scalar!(Nonce, Bytes32);
client_type_scalar!(PublicKey, Bytes64);
client_type_scalar!(Salt, Bytes32);
client_type_scalar!(Signature, Bytes64);
client_type_scalar!(TransactionId, Bytes32);
client_type_scalar!(UtxoId, Bytes33);

impl Copy for Address {}
impl Copy for AssetId {}
impl Copy for BlockId {}
impl Copy for ContractId {}
impl Copy for Hash {}
impl Copy for MerkleRoot {}
impl Copy for MessageId {}
impl Copy for Nonce {}
impl Copy for PublicKey {}
impl Copy for Salt {}
impl Copy for Signature {}
impl Copy for TransactionId {}
impl Copy for UtxoId {}

#[derive(Debug)]
pub struct Tai64Timestamp(pub Tai64);

impl From<Tai64> for Tai64Timestamp {
    fn from(value: Tai64) -> Self {
        Self(value)
    }
}

impl ContractId {
    /// Seed for the calculation of the contract id from its code.
    ///
    /// <https://github.com/FuelLabs/fuel-specs/blob/master/src/protocol/id/contract.md>
    pub const SEED: [u8; 4] = 0x4655454C_u32.to_be_bytes();
}

impl AssetId {
    /// The base native asset of the Fuel protocol.
    pub const BASE: AssetId =
        AssetId::new(<AssetId as ClientScalar>::PrimitiveType::zeroed().0);
}

impl UtxoId {
    pub fn tx_id(&self) -> [u8; 32] {
        let bytes = &self.0 .0 .0;
        let mut tx_id = [0; 32];
        tx_id.copy_from_slice(&bytes[0..32]);
        tx_id
    }

    pub fn output_index(&self) -> u8 {
        self.0 .0 .0[32]
    }
}

impl From<UtxoId> for fuel_core_types::fuel_tx::UtxoId {
    fn from(value: UtxoId) -> Self {
        Self::new(value.tx_id().into(), value.output_index())
    }
}
