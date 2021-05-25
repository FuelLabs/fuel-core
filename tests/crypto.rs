#![feature(once_cell)]

use fuel_tx::crypto as tx_crypto;
use fuel_vm_rust::crypto;
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};
use secp256k1::{PublicKey, Secp256k1, SecretKey};

use std::convert::TryFrom;

pub mod common;

#[test]
fn ecrecover() {
    let secp = Secp256k1::new();
    let mut rng = StdRng::seed_from_u64(2322u64);
    let mut secret_seed = [0u8; 32];
    let mut message = [0u8; 95];

    for _ in 0..10 {
        rng.fill_bytes(&mut message);
        rng.fill_bytes(&mut secret_seed);

        let secret = SecretKey::from_slice(&secret_seed).expect("Failed to generate random secret!");
        let public = PublicKey::from_secret_key(&secp, &secret).serialize_uncompressed();
        let public = <[u8; 64]>::try_from(&public[1..]).expect("Failed to parse public key!");

        let e = tx_crypto::hash(&message);

        let sig =
            crypto::secp256k1_sign_compact_recoverable(secret.as_ref(), &e).expect("Failed to generate signature");
        let pk_p = crypto::secp256k1_sign_compact_recover(&sig, &e).expect("Failed to recover PK from signature");

        assert_eq!(public, pk_p);
    }
}
