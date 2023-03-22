#![allow(missing_docs)]
use ethers_contract::abigen;

pub mod bridge {
    // The `FuelMessagePortal.json` file is auto-generated from contracts:
    // https://github.com/FuelLabs/fuel-v2-contracts
    //
    // To re-generate the file:
    // 1. Download the repository
    // 2. Build the contracts(Check the `README.md` file for details).
    // 3. Copy the content of the `abi` field in the
    //  `artifacts/contracts/fuelchain/FuelMessagePortal.sol/FuelMessagePortal.json`
    super::abigen!(MessageSent, "abi/FuelMessagePortal.json");
}
