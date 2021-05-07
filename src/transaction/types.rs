mod input;
mod output;
mod witness;

pub type Color = [u8; 32];
pub type Id = [u8; 32];
pub type Root = [u8; 32];

pub use input::Input;
pub use output::Output;
pub use witness::Witness;
