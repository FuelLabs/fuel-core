pub mod client;
#[cfg(feature = "dap")]
pub mod schema;

// re-exports
#[doc(no_inline)]
pub use fuel_tx;
#[doc(no_inline)]
pub use fuel_types;
#[doc(no_inline)]
pub use fuel_vm;
