use ethers_contract::abigen;

pub mod fuel {
    super::abigen!(Fuel, "abi/Fuel.json");
}
pub mod validators {
    super::abigen!(ValidatorSet, "abi/ValidatorSet.json");
}

pub use fuel::Fuel;
pub use validators::ValidatorSet;
