use fuel_core::consts::*;
use fuel_core::crypto;
use fuel_core::prelude::*;
use fuel_tx::crypto as tx_crypto;

use std::convert::TryFrom;
use std::str::FromStr;

#[test]
fn ecrecover() {
    use secp256k1::{PublicKey, Secp256k1, SecretKey};

    let secp = Secp256k1::new();
    let secret = SecretKey::from_str("3b940b5586823dfd02ae3b461bb4336b5ecbaefd6627aa922efc048fec0c881c").unwrap();
    let public = PublicKey::from_secret_key(&secp, &secret).serialize_uncompressed();
    let public = <[u8; 64]>::try_from(&public[1..]).expect("Failed to parse public key!");

    let message = b"The gift of words is the gift of deception and illusion.";
    let e = tx_crypto::hash(&message[..]);
    let sig = crypto::secp256k1_sign_compact_recoverable(secret.as_ref(), &e).expect("Failed to generate signature");

    let mut vm = Interpreter::default();
    vm.init(Transaction::default()).expect("Failed to init VM");

    // r[0x10] := 256
    vm.execute(Opcode::ADDI(0x10, 0x10, 288)).unwrap();
    vm.execute(Opcode::ALOC(0x10)).unwrap();

    // r[0x12] := r[hp]
    vm.execute(Opcode::MOVE(0x12, 0x07)).unwrap();

    // m[hp + 1..hp + |e| + |sig| + |public| + 1] <- e + sig + public
    e.iter()
        .chain(sig.iter())
        .chain(public.iter())
        .enumerate()
        .for_each(|(i, b)| {
            vm.execute(Opcode::ADDI(0x11, 0x13, (*b) as Immediate12)).unwrap();
            vm.execute(Opcode::SB(0x12, 0x11, (i + 1) as Immediate12)).unwrap();
        });

    // Set e address to 0x11
    // r[0x11] := r[hp] + 1
    vm.execute(Opcode::ADDI(0x11, 0x12, 1)).unwrap();

    // Set sig address to 0x14
    // r[0x14] := r[0x11] + |e|
    vm.execute(Opcode::ADDI(0x14, 0x11, e.len() as Immediate12)).unwrap();

    // Set public key address to 0x15
    // r[0x15] := r[0x14] + |sig|
    vm.execute(Opcode::ADDI(0x15, 0x14, sig.len() as Immediate12)).unwrap();

    // Set calculated public key address to 0x16
    // r[0x16] := r[0x15] + |public|
    vm.execute(Opcode::ADDI(0x16, 0x15, public.len() as Immediate12))
        .unwrap();

    // Compute the ECRECOVER
    vm.execute(Opcode::ECR(0x16, 0x14, 0x11)).unwrap();

    // r[0x18] := |recover|
    // r[0x17] := m[public key] == m[recovered public key]
    vm.execute(Opcode::ADDI(0x18, 0x18, public.len() as Immediate12))
        .unwrap();
    vm.execute(Opcode::MEQ(0x17, 0x15, 0x16, 0x18)).unwrap();
    assert_eq!(0x01, vm.registers()[0x17]);

    // Corrupt the signature
    // r[0x19] := 0xff
    // m[sig] := r[0x19]
    vm.execute(Opcode::ADDI(0x19, 0x19, 0xff)).unwrap();
    vm.execute(Opcode::SB(0x14, 0x19, 0)).unwrap();

    // Set calculated corrupt public key address to 0x1a
    // r[0x1a] := r[0x16] + |public|
    vm.execute(Opcode::ADDI(0x1a, 0x16, public.len() as Immediate12))
        .unwrap();

    // Compute the ECRECOVER with a corrupted signature
    vm.execute(Opcode::ECR(0x1a, 0x14, 0x11)).unwrap();

    // r[0x17] := m[public key] == m[recovered public key]
    // r[0x17] must be false
    vm.execute(Opcode::MEQ(0x17, 0x15, 0x1a, 0x18)).unwrap();
    assert_eq!(0x00, vm.registers()[0x17]);
    assert_eq!(1, vm.registers()[REG_ERR]);
}

#[test]
fn sha256() {
    let message = b"I say let the world go to hell, but I should always have my tea.";
    let hash = tx_crypto::hash(message);

    let mut vm = Interpreter::default();
    vm.init(Transaction::default()).expect("Failed to init VM");

    // r[0x10] := 128
    vm.execute(Opcode::ADDI(0x10, 0x10, 128)).unwrap();
    vm.execute(Opcode::ALOC(0x10)).unwrap();

    // r[0x12] := r[hp]
    vm.execute(Opcode::MOVE(0x12, 0x07)).unwrap();

    // m[hp + 1..hp + |message| + |hash| + 1] <- message + hash
    message.iter().chain(hash.iter()).enumerate().for_each(|(i, b)| {
        vm.execute(Opcode::ADDI(0x11, 0x13, (*b) as Immediate12)).unwrap();
        vm.execute(Opcode::SB(0x12, 0x11, (i + 1) as Immediate12)).unwrap();
    });

    // Set message address to 0x11
    // r[0x11] := r[hp] + 1
    vm.execute(Opcode::ADDI(0x11, 0x12, 1)).unwrap();

    // Set hash address to 0x14
    // r[0x14] := r[0x11] + |message|
    vm.execute(Opcode::ADDI(0x14, 0x11, message.len() as Immediate12))
        .unwrap();

    // Set calculated hash address to 0x15
    // r[0x15] := r[0x14] + |hash|
    vm.execute(Opcode::ADDI(0x15, 0x14, hash.len() as Immediate12)).unwrap();

    // Set hash size
    // r[0x16] := |message|
    vm.execute(Opcode::ADDI(0x16, 0x16, message.len() as Immediate12))
        .unwrap();

    // Compute the SHA256
    vm.execute(Opcode::S256(0x15, 0x11, 0x16)).unwrap();

    // r[0x18] := |hash|
    // r[0x17] := m[hash] == m[computed hash]
    vm.execute(Opcode::ADDI(0x18, 0x18, hash.len() as Immediate12)).unwrap();
    vm.execute(Opcode::MEQ(0x17, 0x14, 0x15, 0x18)).unwrap();
    assert_eq!(0x01, vm.registers()[0x17]);

    // Corrupt the message
    // r[0x19] := 0xff
    // m[message] := r[0x19]
    vm.execute(Opcode::ADDI(0x19, 0x19, 0xff)).unwrap();
    vm.execute(Opcode::SB(0x11, 0x19, 0)).unwrap();

    // Compute the SHA256 of the corrupted message
    vm.execute(Opcode::S256(0x15, 0x11, 0x16)).unwrap();

    // r[0x17] := m[hash] == m[computed hash]
    // r[0x17] must be false
    vm.execute(Opcode::MEQ(0x17, 0x14, 0x15, 0x18)).unwrap();
    assert_eq!(0x00, vm.registers()[0x17]);
}

#[test]
fn keccak256() {
    use sha3::{Digest, Keccak256};

    let message = b"...and, moreover, I consider it my duty to warn you that the cat is an ancient, inviolable animal.";
    let mut hasher = Keccak256::new();
    hasher.update(message);
    let hash = hasher.finalize();

    let mut vm = Interpreter::default();
    vm.init(Transaction::default()).expect("Failed to init VM");

    // r[0x10] := 162
    vm.execute(Opcode::ADDI(0x10, 0x10, 162)).unwrap();
    vm.execute(Opcode::ALOC(0x10)).unwrap();

    // r[0x12] := r[hp]
    vm.execute(Opcode::MOVE(0x12, 0x07)).unwrap();

    // m[hp + 1..hp + |message| + |hash| + 1] <- message + hash
    message.iter().chain(hash.iter()).enumerate().for_each(|(i, b)| {
        vm.execute(Opcode::ADDI(0x11, 0x13, (*b) as Immediate12)).unwrap();
        vm.execute(Opcode::SB(0x12, 0x11, (i + 1) as Immediate12)).unwrap();
    });

    // Set message address to 0x11
    // r[0x11] := r[hp] + 1
    vm.execute(Opcode::ADDI(0x11, 0x12, 1)).unwrap();

    // Set hash address to 0x14
    // r[0x14] := r[0x11] + |message|
    vm.execute(Opcode::ADDI(0x14, 0x11, message.len() as Immediate12))
        .unwrap();

    // Set calculated hash address to 0x15
    // r[0x15] := r[0x14] + |hash|
    vm.execute(Opcode::ADDI(0x15, 0x14, hash.len() as Immediate12)).unwrap();

    // Set hash size
    // r[0x16] := |message|
    vm.execute(Opcode::ADDI(0x16, 0x16, message.len() as Immediate12))
        .unwrap();

    // Compute the Keccak256
    vm.execute(Opcode::K256(0x15, 0x11, 0x16)).unwrap();

    // r[0x18] := |hash|
    // r[0x17] := m[hash] == m[computed hash]
    vm.execute(Opcode::ADDI(0x18, 0x18, hash.len() as Immediate12)).unwrap();
    vm.execute(Opcode::MEQ(0x17, 0x14, 0x15, 0x18)).unwrap();
    assert_eq!(0x01, vm.registers()[0x17]);

    // Corrupt the message
    // r[0x19] := 0xff
    // m[message] := r[0x19]
    vm.execute(Opcode::ADDI(0x19, 0x19, 0xff)).unwrap();
    vm.execute(Opcode::SB(0x11, 0x19, 0)).unwrap();

    // Compute the Keccak256 of the corrupted message
    vm.execute(Opcode::K256(0x15, 0x11, 0x16)).unwrap();

    // r[0x17] := m[hash] == m[computed hash]
    // r[0x17] must be false
    vm.execute(Opcode::MEQ(0x17, 0x14, 0x15, 0x18)).unwrap();
    assert_eq!(0x00, vm.registers()[0x17]);
}
