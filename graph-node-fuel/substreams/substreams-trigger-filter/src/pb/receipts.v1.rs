#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BlockAndReceipts {
    #[prost(message, optional, tag = "1")]
    pub block: ::core::option::Option<
        ::substreams_near_core::pb::sf::near::r#type::v1::Block,
    >,
    #[prost(message, repeated, tag = "2")]
    pub outcome: ::prost::alloc::vec::Vec<
        ::substreams_near_core::pb::sf::near::r#type::v1::ExecutionOutcomeWithId,
    >,
    #[prost(message, repeated, tag = "3")]
    pub receipt: ::prost::alloc::vec::Vec<
        ::substreams_near_core::pb::sf::near::r#type::v1::Receipt,
    >,
}
