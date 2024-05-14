#![allow(missing_docs)]
use ethers_contract::abigen;

pub mod bridge {
    // The link to the original event definition:
    // https://github.com/FuelLabs/fuel-bridge/blob/05c4d9cced70d262742e20c85c7ef8a5d8898701/packages/portal-contracts/contracts/fuelchain/FuelMessagePortal.sol#L54
    super::abigen!(
        MessageSent,
        r#"[
            event MessageSent(bytes32 indexed sender, bytes32 indexed recipient, uint256 indexed nonce, uint64 amount, bytes data)
            event Transaction(uint256 indexed nonce, uint64 max_gas, bytes canonically_serialized_tx)
        ]"#,
    );
}
