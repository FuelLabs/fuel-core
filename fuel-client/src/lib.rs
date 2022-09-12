pub mod client;
#[cfg(feature = "dap")]
pub mod schema;

// re-exports
#[doc(no_inline)]
pub use fuel_vm;
#[doc(no_inline)]
pub use fuel_vm::*;
