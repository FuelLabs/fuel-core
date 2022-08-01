use ethers_contract::abigen;

pub mod fuel {
    super::abigen!(Fuel, "abi/Fuel.json");
}
pub mod validators {
    super::abigen!(ValidatorSet, "abi/ValidatorSet.json");
}

pub mod bridge {
    super::abigen!(Message, "abi/IFuelMessageOutbox.json");
}

pub use bridge::Message;
pub use fuel::Fuel;
pub use validators::ValidatorSet;
