#![no_std]
extern crate alloc;
use core::convert::TryFrom;
use fuel_indexer_derive::{graphql_schema, handler};
use alloc::vec::Vec;
use fuel_indexer::types::*;
use fuels_core::{ParamType, Token};

graphql_schema!("namespace", "doesnt_exist.graphql");

struct SomeEvent {
    id: u64,
    account: Address,
}

impl SomeEvent {
    fn param_types() -> Vec<ParamType> {
        Vec::new()
    }
    pub fn into_token(self) -> Token {
        Token::Struct(Vec::new())
    }
    pub fn new_from_tokens(tokens: &[Token]) -> SomeEvent {
        SomeEvent {
            id: 4,
            account: Address::default(),
        }
    }
}

#[handler]
fn function_one(event: SomeEvent) {
    let SomeEvent { id, account } = event;

    assert_eq!(id, 0);
    assert_eq!(account, Address::try_from([0; 32]).expect("failed"));
}

fn main() {
    use fuels_core::abi_encoder::ABIEncoder;
    let s = SomeEvent {
        id: 0,
        account: Address::try_from([0; 32]).expect("failed"),
    };

    let mut bytes = ABIEncoder::new().encode(&[s.into_token()]).expect("Failed compile test");

    let ptr = bytes.as_mut_ptr();
    let len = bytes.len();
    core::mem::forget(bytes);

    function_one(ptr, len);
}
