use fuel_tx::{crypto as tx_crypto, Hash};
use secp256k1::recovery::{RecoverableSignature, RecoveryId};
use secp256k1::Error as Secp256k1Error;
use secp256k1::{Message, Secp256k1, SecretKey};

use std::convert::TryFrom;

/// Sign a given message and compress the `v` to the signature
///
/// The compression scheme is described in
/// https://github.com/lazyledger/lazyledger-specs/blob/master/specs/data_structures.md#public-key-cryptography
pub fn secp256k1_sign_compact_recoverable(secret: &[u8], message: &[u8]) -> Result<[u8; 64], Secp256k1Error> {
    let secret = SecretKey::from_slice(secret)?;
    let message = Message::from_slice(message)?;

    let signature = Secp256k1::new().sign_recoverable(&message, &secret);
    let (v, mut signature) = signature.serialize_compact();

    let v = v.to_i32();
    signature[32] |= (v << 7) as u8;

    Ok(signature)
}

/// Recover the public key from a signature performed with
/// [`secp256k1_sign_compact_recoverable`]
pub fn secp256k1_sign_compact_recover(signature: &[u8], message: &[u8]) -> Result<[u8; 64], Secp256k1Error> {
    let message = Message::from_slice(message)?;
    let mut signature = <[u8; 64]>::try_from(signature).map_err(|_| Secp256k1Error::InvalidSignature)?;

    let v = ((signature[32] & 0x80) >> 7) as i32;
    signature[32] &= 0x7f;

    let v = RecoveryId::from_i32(v)?;
    let signature = RecoverableSignature::from_compact(&signature, v)?;

    let pk = Secp256k1::new().recover(&message, &signature)?.serialize_uncompressed();

    // Ignore the first byte of the compressed flag
    <[u8; 64]>::try_from(&pk[1..]).map_err(|_| Secp256k1Error::InvalidPublicKey)
}

pub fn merkle_root(data: &[u8]) -> Hash {
    // TODO implement merkle root
    tx_crypto::hash(data)
}
