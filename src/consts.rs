/* MEMORY TYPES */

// memory width
pub type MemWord = u64;

// max sizes in u64 words
// pub const FUEL_MAX_MEMORY_SIZE: usize = 32 * /* MB */ 1024 * /* KB */ 1024;
// use a small size for now
pub const FUEL_MAX_MEMORY_SIZE: u8 = 64;

// constraints for program input
// pub const FUEL_MAX_PROGRAM_SIZE: usize = 16 * /* KB */ 1024;
// use a small size for now
pub const FUEL_MAX_PROGRAM_SIZE: u8 = 16;

// no limits to heap for now.

/* FLAG AND REGISTER TYPES */

// register count for checking constraints
pub const VM_REGISTER_COUNT: u8 = 64;

// register-based addressing for 32MB of memory in bytecode-land
// used for serder
pub const VM_REGISTER_WIDTH: u8 = 6;

// internal representation for register ids
// simpler to represent as usize since it avoids casts
pub type RegisterId = u8;
