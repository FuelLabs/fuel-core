#![deny(clippy::cast_possible_truncation)]
#![deny(clippy::arithmetic_side_effects)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

pub mod config;
mod genesis;
mod serialization;

pub use config::*;
use fuel_core_types::{
    fuel_asm::{
        op,
        RegId,
    },
    fuel_tx::Address,
    fuel_vm::{
        CallFrame,
        SecretKey,
    },
};
pub use genesis::GenesisCommitment;

/// A default secret key to use for testing purposes only
pub fn default_consensus_dev_key() -> SecretKey {
    // Derived from:
    //  - Mnemonic phrase: "winner alley monkey elephant sun off boil hope toward boss bronze dish"
    //  - Path: "m/44'/60'/0'/0/0"
    // Equivalent to:
    //  `SecretKey::new_from_mnemonic_phrase_with_path(..)`
    let bytes: [u8; 32] = [
        0xfb, 0xe4, 0x91, 0x78, 0xda, 0xc2, 0xdf, 0x5f, 0xde, 0xa7, 0x4a, 0x11, 0xa9,
        0x0f, 0x99, 0x77, 0x62, 0x5f, 0xe0, 0x23, 0xcd, 0xf6, 0x41, 0x4b, 0xfd, 0x63,
        0x9d, 0x32, 0x7a, 0x2e, 0x9d, 0xdb,
    ];
    SecretKey::try_from(bytes.as_slice()).expect("valid key")
}

pub fn generate_fee_collection_contract(address: Address) -> Vec<u8> {
    // TODO: Tests to write for this contract:
    // 1. Happy path withdrawal case for block producer
    //    Deploy the contract for the block producer's address
    //    Run some blocks and accumulate fees
    //    Attempt to withdraw the collected fees to the BP's address
    //      (note that currently we allow anyone to initiate this withdrawal)
    // 2. Unhappy case where tx doesn't have the expected variable output set
    // 3. Edge case, withdrawal is attempted when there are no fees collected (shouldn't revert, but the BP's balance should be the same)

    // TODO: setup cli interface for generating this contract
    // This should be accessible to the fuel-core CLI.
    // ie. something like `fuel-core generate-fee-contract <WITHDRAWAL_ADDRESS>` -> <CONTRACT_BYTECODE_HEX>

    let asm = vec![
        // Pointer to AssetID memory address in call frame param a
        op::addi(0x10, RegId::FP, CallFrame::a_offset().try_into().unwrap()),
        // pointer to the withdrawal address embedded after the contract bytecode
        op::addi(
            0x11,
            RegId::IS,
            7, // update when number of opcodes changes
        ),
        // get the balance of asset ID in the contract
        op::bal(0x11, 0x10, RegId::FP),
        // if balance > 0, withdraw
        op::eq(0x12, 0x11, RegId::ZERO),
        op::jnzf(0x12, RegId::ZERO, 1),
        // todo: point $rA to the location of the withdrawal address in memory (after return)
        op::tro(0, 0, 0x11, 0x10),
        // return
        op::ret(RegId::ONE),
    ];

    let mut asm_bytes: Vec<u8> = asm.into_iter().collect();
    // append withdrawal address at the end of the contract bytecode.
    asm_bytes.extend_from_slice(address.as_slice());

    asm_bytes
}
