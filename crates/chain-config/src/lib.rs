#![deny(unused_crate_dependencies)]
#![deny(warnings)]

pub mod config;
mod genesis;
mod serialization;

pub use config::*;
use fuel_core_types::fuel_vm::SecretKey;
pub use genesis::GenesisCommitment;

/// A default secret key to use for testing purposes only
pub fn default_consensus_dev_key() -> SecretKey {
    const DEV_KEY_PHRASE: &str =
        "winner alley monkey elephant sun off boil hope toward boss bronze dish";
    SecretKey::new_from_mnemonic_phrase_with_path(DEV_KEY_PHRASE, "m/44'/60'/0'/0/0")
        .expect("valid key")
}
