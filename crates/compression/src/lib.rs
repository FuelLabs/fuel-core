mod compression;
mod registry;

pub use compression::{
    Compactable,
    CompactionContext,
};
pub use registry::{
    tables,
    ChangesPerTable,
    CountPerTable,
    Key,
    RegistryDb,
    Table,
};

#[cfg(feature = "test-helpers")]
pub use registry::in_memory::InMemoryRegistry;

pub use fuel_core_compression_derive::Compact;
