use crate::client::{
    schema,
    types::scalars,
};
use fuel_core_types::{
    fuel_tx::input::AsField,
    fuel_types::Bytes64,
};

macro_rules! translate_client_type_scalar {
    ($client_type:ident, $from_type:ident) => {
        impl From<schema::$from_type> for scalars::$client_type {
            fn from(value: schema::$from_type) -> Self {
                let bytes: <scalars::$client_type as scalars::ClientScalar>::PrimitiveType =
                    value.0 .0.into();
                bytes.into()
            }
        }


    };
}

translate_client_type_scalar!(Address, Address);
translate_client_type_scalar!(AssetId, AssetId);
translate_client_type_scalar!(BlockId, BlockId);
translate_client_type_scalar!(ContractId, ContractId);
translate_client_type_scalar!(Hash, Bytes32);
translate_client_type_scalar!(HexString, HexString);
translate_client_type_scalar!(MerkleRoot, Bytes32);
translate_client_type_scalar!(MessageId, MessageId);
translate_client_type_scalar!(Nonce, Nonce);
translate_client_type_scalar!(Salt, Salt);
translate_client_type_scalar!(Signature, Signature);
translate_client_type_scalar!(TransactionId, TransactionId);

impl From<schema::UtxoId> for scalars::UtxoId {
    fn from(value: schema::UtxoId) -> Self {
        let utxo_id = value.0 .0;
        let tx_id: [u8; 32] = (*utxo_id.tx_id()).into();
        let output_index: u8 = utxo_id.output_index();
        let mut bytes: [u8; 33] = [0; 33];
        bytes[0..32].copy_from_slice(&tx_id);
        bytes[32] = output_index;
        Self::new(bytes)
    }
}

macro_rules! translate_scalar_to_fuel_tx {
    ($client_type:ident, $to_type:ident) => {
        impl From<scalars::$client_type> for fuel_core_types::fuel_types::$to_type {
            fn from(value: scalars::$client_type) -> Self {
                let bytes = value.0 .0 .0;
                bytes.into()
            }
        }
    };
}

translate_scalar_to_fuel_tx!(Address, Address);
translate_scalar_to_fuel_tx!(AssetId, AssetId);
translate_scalar_to_fuel_tx!(MessageId, MessageId);
translate_scalar_to_fuel_tx!(Nonce, Nonce);

impl From<fuel_core_types::fuel_crypto::PublicKey> for scalars::PublicKey {
    fn from(value: fuel_core_types::fuel_crypto::PublicKey) -> Self {
        let bytes = *value;
        Self::new(bytes)
    }
}
