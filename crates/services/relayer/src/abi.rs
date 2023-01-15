#![allow(missing_docs)]
use ethers_contract::abigen;

pub mod bridge {
    super::abigen!(Message, "abi/FuelMessagePortal.json");
}

pub use bridge::Message;
