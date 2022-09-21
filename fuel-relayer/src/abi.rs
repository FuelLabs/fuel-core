#![allow(missing_docs)]
use ethers_contract::abigen;

pub mod fuel {
    super::abigen!(Fuel, "abi/FuelSidechain.json");
}
pub mod bridge {
    super::abigen!(Message, "abi/FuelMessagePortal.json");
}

pub use bridge::Message;
pub use fuel::Fuel;
