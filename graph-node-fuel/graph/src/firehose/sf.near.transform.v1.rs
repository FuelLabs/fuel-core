#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BasicReceiptFilter {
    #[prost(string, repeated, tag = "1")]
    pub accounts: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(message, repeated, tag = "2")]
    pub prefix_and_suffix_pairs: ::prost::alloc::vec::Vec<PrefixSuffixPair>,
}
/// PrefixSuffixPair applies a logical AND to prefix and suffix when both fields are non-empty.
/// * {prefix="hello",suffix="world"} will match "hello.world" but not "hello.friend"
/// * {prefix="hello",suffix=""}      will match both "hello.world" and "hello.friend"
/// * {prefix="",suffix="world"}      will match both "hello.world" and "good.day.world"
/// * {prefix="",suffix=""}           is invalid
///
/// Note that the suffix will usually have a TLD, ex: "mydomain.near" or "mydomain.testnet"
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PrefixSuffixPair {
    #[prost(string, tag = "1")]
    pub prefix: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub suffix: ::prost::alloc::string::String,
}
