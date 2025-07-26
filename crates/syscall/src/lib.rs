//! The crate `fuel-core-syscall` contains the syscall handler and types for fuel-core.
//! It also exposes supported syscall numbers and parameters as constants.

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(missing_docs)]
#![deny(warnings)]

#[cfg(feature = "alloc")]
extern crate alloc;

/// Syscall handlers and types.
pub mod handlers;
