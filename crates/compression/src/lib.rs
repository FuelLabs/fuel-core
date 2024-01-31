mod compression;
mod registry;

pub use compression::{
    Compactable,
    CompactionContext,
};
pub use registry::{
    db,
    tables,
    ChangesPerTable,
    CountPerTable,
    Key,
    Table,
};

#[cfg(feature = "test-helpers")]
pub use registry::in_memory::InMemoryRegistry;

pub use fuel_core_compression_derive::Compact;
