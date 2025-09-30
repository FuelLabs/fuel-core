#![allow(missing_docs)]
use alloy_sol_types::sol;

pub mod bridge {
    super::sol! {
        event MessageSent(
            bytes32 indexed sender,
            bytes32 indexed recipient,
            uint256 indexed nonce,
            uint64 amount,
            bytes data
        );

        event Transaction(
            uint256 indexed nonce,
            uint64 max_gas,
            bytes canonically_serialized_tx
        );
    }
}
