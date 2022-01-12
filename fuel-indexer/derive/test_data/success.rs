#![no_std]
extern crate alloc;
use core::convert::TryFrom;
use fuel_indexer_derive::handler;
use alloc::vec::Vec;
use fuel_indexer::types::*;
use fuels_core::{ParamType, Token};

struct SomeEvent {
    id: u64,
    account: Address,
}

impl SomeEvent {
    fn param_types() -> Vec<ParamType> {
        let mut t = Vec::new();
        t.push(ParamType::U64);
        t.push(ParamType::B256);
        t
    }

    fn into_token(self) -> Token {
        let mut t = Vec::new();
        t.push(Token::U64(self.id));
        t.push(Token::B256(self.account.into()));
        Token::Struct(t)
    }

    fn new_from_tokens(tokens: &[Token]) -> SomeEvent {
        let id = match tokens[0] {
            Token::U64(i) => i,
            _ => panic!("Should be a U64"),
        };

        let addr = match tokens[1] {
            Token::B256(b) => b,
            _ => panic!("Should be a U256"),
        };

        SomeEvent { id, account: Address::from(addr) }
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
