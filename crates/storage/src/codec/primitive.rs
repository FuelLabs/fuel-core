//! The module contains the implementation of the `Postcard` codec.
//! The codec is used for types that can be represented by an array.
//! It includes all primitive types and types that are arrays inside
//! or could be represented by arrays.

use crate::codec::{
    Decode,
    Encode,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_tx::{
        TxId,
        UtxoId,
    },
    fuel_types::BlockHeight,
};

/// The codec is used for types that can be represented by an array.
/// The `SIZE` const specifies the size of the array used to represent the type.
pub struct Primitive<const SIZE: usize>;

macro_rules! impl_encode {
    ($($ty:ty, $size:expr),*) => {
        $(
            impl Encode<$ty> for Primitive<{ $size }> {
                type Encoder<'a> = [u8; { $size }];

                fn encode(t: &$ty) -> Self::Encoder<'_> {
                    t.to_be_bytes()
                }
            }
        )*
    };
}
macro_rules! impl_decode {
    ($($ty:ty, $size:expr),*) => {
        $(
            impl Decode<$ty> for Primitive<{ $size }> {
                fn decode(bytes: &[u8]) -> anyhow::Result<$ty> {
                    Ok(<$ty>::from_be_bytes(<[u8; { $size }]>::try_from(bytes)?))
                }
            }
        )*
    };
}

impl_encode! {
    u8, 1,
    u16, 2,
    u32, 4,
    BlockHeight, 4,
    DaBlockHeight, 8,
    u64, 8,
    u128, 16
}

impl_decode! {
    u8, 1,
    u16, 2,
    u32, 4,
    u64, 8,
    u128, 16
}

impl Decode<BlockHeight> for Primitive<4> {
    fn decode(bytes: &[u8]) -> anyhow::Result<BlockHeight> {
        Ok(BlockHeight::from(<[u8; 4]>::try_from(bytes)?))
    }
}

impl Decode<DaBlockHeight> for Primitive<8> {
    fn decode(bytes: &[u8]) -> anyhow::Result<DaBlockHeight> {
        Ok(DaBlockHeight::from(<[u8; 8]>::try_from(bytes)?))
    }
}

/// Converts the `UtxoId` into an array of bytes.
pub fn utxo_id_to_bytes(utxo_id: &UtxoId) -> [u8; TxId::LEN + 2] {
    let mut default = [0; TxId::LEN + 2];
    default[0..TxId::LEN].copy_from_slice(utxo_id.tx_id().as_ref());
    default[TxId::LEN..].copy_from_slice(utxo_id.output_index().to_be_bytes().as_slice());
    default
}

impl Encode<UtxoId> for Primitive<{ TxId::LEN + 2 }> {
    type Encoder<'a> = [u8; TxId::LEN + 2];

    fn encode(t: &UtxoId) -> Self::Encoder<'_> {
        utxo_id_to_bytes(t)
    }
}

impl Decode<UtxoId> for Primitive<{ TxId::LEN + 2 }> {
    fn decode(bytes: &[u8]) -> anyhow::Result<UtxoId> {
        let bytes = <[u8; TxId::LEN + 2]>::try_from(bytes)?;
        let tx_id: [u8; TxId::LEN] = bytes[0..TxId::LEN].try_into()?;
        let output_index = u16::from_be_bytes(bytes[TxId::LEN..].try_into()?);
        Ok(UtxoId::new(TxId::from(tx_id), output_index))
    }
}
