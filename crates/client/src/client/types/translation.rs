use crate::client::{
    schema,
    types::scalars,
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
translate_client_type_scalar!(UtxoId, UtxoId);
