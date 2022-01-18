pub use no_name_mod::*;
#[allow(clippy::too_many_arguments)]
mod no_name_mod {
    #![allow(clippy::enum_variant_names)]
    #![allow(dead_code)]
    #![allow(unused_imports)]
    use alloc::vec::Vec;
    use fuels_core::{EnumSelector, ParamType, Token, Tokenizable};
    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub struct SomeEvent {
        pub id: u64,
        pub account: [u8; 32],
    }
    impl SomeEvent {
        pub fn param_types() -> Vec<ParamType> {
            vec![ParamType::U64, ParamType::B256]
        }
        pub fn into_token(self) -> Token {
            Token::Struct(vec![Token::U64(self.id), Token::B256(self.account)])
        }
        pub fn new_from_token(tokens: &[Token]) -> Self {
            Self { id : < u64 > :: from_token (tokens [0usize] . clone ()) . expect ("Failed to run `new_from_token()` for custom struct, make sure to pass tokens in the right order and right types") , account : < [u8 ; 32] > :: from_token (tokens [1usize] . clone ()) . expect ("Failed to run `new_from_token()` for custom struct, make sure to pass tokens in the right order and right types") }
        }
    }
    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub struct AnotherEvent {
        pub id: u64,
        pub hash: [u8; 32],
        pub bar: bool,
    }
    impl AnotherEvent {
        pub fn param_types() -> Vec<ParamType> {
            vec![ParamType::U64, ParamType::B256, ParamType::Bool]
        }
        pub fn into_token(self) -> Token {
            Token::Struct(vec![Token::U64(self.id), Token::B256(self.hash), Token::Bool(self.bar)])
        }
        pub fn new_from_token(tokens: &[Token]) -> Self {
            Self { id : < u64 > :: from_token (tokens [0usize] . clone ()) . expect ("Failed to run `new_from_token()` for custom struct, make sure to pass tokens in the right order and right types") , hash : < [u8 ; 32] > :: from_token (tokens [1usize] . clone ()) . expect ("Failed to run `new_from_token()` for custom struct, make sure to pass tokens in the right order and right types") , bar : < bool > :: from_token (tokens [2usize] . clone ()) . expect ("Failed to run `new_from_token()` for custom struct, make sure to pass tokens in the right order and right types") }
        }
    }
}
