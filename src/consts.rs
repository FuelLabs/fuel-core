/* MEMORY TYPES */

/// Maximum contract size, in bytes.
pub const CONTRACT_MAX_SIZE: u64 = 16 * 1024;

/// Maximum memory access size, in bytes.
pub const MEM_MAX_ACCESS_SIZE: u64 = 32 * 1024 * 1024;

/// Maximum VM RAM, in bytes.
pub const VM_MAX_RAM: u64 = 1024 * 1024;

/* FLAG AND REGISTER TYPES */

/// Register count for checking constraints
pub const VM_REGISTER_COUNT: usize = 64;

/* TRANSACTION VALUES */

/// Maximum number of inputs.
pub const MAX_INPUTS: u64 = 8;

/* END */

// max sizes in u64 words
// pub const FUEL_MAX_MEMORY_SIZE: usize = 32 * /* MB */ 1024 * /* KB */ 1024;
// use a small size for now
pub const FUEL_MAX_MEMORY_SIZE: u8 = 64;

// constraints for program input
// pub const FUEL_MAX_PROGRAM_SIZE: usize = 16 * /* KB */ 1024;
// use a small size for now
pub const FUEL_MAX_PROGRAM_SIZE: u8 = 16;

// no limits to heap for now.

// register-based addressing for 32MB of memory in bytecode-land
// used for serder
pub const VM_REGISTER_WIDTH: u8 = 6;
