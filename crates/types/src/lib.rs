//! The crate `fuel-core-types` contains plain rust common type used by `fuel-core` and related
//! libraries. This crate doesn't contain any business logic and is to be such primitive as that
//! is possible.

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(missing_docs)]
#![deny(warnings)]

#[cfg(feature = "alloc")]
extern crate alloc;

#[doc(no_inline)]
pub use fuel_vm_private::{
    fuel_asm,
    fuel_crypto,
    fuel_merkle,
    fuel_tx,
    fuel_types,
};
#[doc(no_inline)]
pub use secrecy;
#[doc(no_inline)]
pub use tai64;

pub mod blockchain;
pub mod entities;
pub mod services;

/// Re-export of some fuel-vm types
pub mod fuel_vm {
    #[doc(no_inline)]
    pub use fuel_vm_private::prelude::Breakpoint;

    #[doc(no_inline)]
    pub use fuel_vm_private::{
        checked_transaction,
        constraints,
        consts,
        crypto,
        double_key,
        error::PredicateVerificationFailed,
        interpreter,
        pool::VmMemoryPool,
        prelude::{
            Backtrace,
            Call,
            CallFrame,
            Contract,
            Interpreter,
            InterpreterError,
            InterpreterStorage,
            PredicateStorage,
            ProgramState,
            Salt,
            SecretKey,
            Signature,
            Transactor,
        },
        script_with_data_offset,
        state,
        storage::ContractsAssetKey,
        storage::ContractsStateKey,
        storage::UploadedBytecode,
        storage::{
            BlobBytes,
            BlobData,
        },
        util,
    };
}
