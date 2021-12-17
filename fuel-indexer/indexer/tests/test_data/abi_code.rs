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
            let mut types = Vec::new();
            types.push(ParamType::U64);
            types.push(ParamType::B256);
            types
        }
        pub fn into_token(self) -> Token {
            let mut tokens = Vec::new();
            tokens.push(Token::U64(self.id));
            tokens.push(Token::B256(self.account));
            Token::Struct(tokens)
        }
        pub fn new_from_tokens(tokens: &[Token]) -> Self {
            Self { id : < u64 > :: from_token (tokens [0usize] . clone ()) . expect ("Failed to run `new_from_tokens()` for custom struct, make sure to pass tokens in the right order and right types") , account : < [u8 ; 32] > :: from_token (tokens [1usize] . clone ()) . expect ("Failed to run `new_from_tokens()` for custom struct, make sure to pass tokens in the right order and right types") }
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
            let mut types = Vec::new();
            types.push(ParamType::U64);
            types.push(ParamType::B256);
            types.push(ParamType::Bool);
            types
        }
        pub fn into_token(self) -> Token {
            let mut tokens = Vec::new();
            tokens.push(Token::U64(self.id));
            tokens.push(Token::B256(self.hash));
            tokens.push(Token::Bool(self.bar));
            Token::Struct(tokens)
        }
        pub fn new_from_tokens(tokens: &[Token]) -> Self {
            Self { id : < u64 > :: from_token (tokens [0usize] . clone ()) . expect ("Failed to run `new_from_tokens()` for custom struct, make sure to pass tokens in the right order and right types") , hash : < [u8 ; 32] > :: from_token (tokens [1usize] . clone ()) . expect ("Failed to run `new_from_tokens()` for custom struct, make sure to pass tokens in the right order and right types") , bar : < bool > :: from_token (tokens [2usize] . clone ()) . expect ("Failed to run `new_from_tokens()` for custom struct, make sure to pass tokens in the right order and right types") }
        }
    }
}
