//! The crate `fuel-core-types` contains plain rust common type used by `fuel-core` and related
//! libraries. This crate doesn't contain any business logic and is to be such primitive as that
//! is possible.

// TODO: Add documentation to all public types
// #![deny(missing_docs)]

pub use fuel_vm_private::{
    fuel_asm,
    fuel_crypto,
    fuel_merkle,
    fuel_tx,
    fuel_types,
};

pub mod blockchain;
pub mod entities;
pub mod services;

pub mod fuel_vm {
    pub use fuel_vm_private::prelude::{
        Interpreter,
        PredicateStorage,
        ProgramState,
    };
}
