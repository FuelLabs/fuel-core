#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BlockAndReceipts {
    #[prost(message, optional, tag = "1")]
    pub block: ::core::option::Option<crate::codec::pbcodec::Block>,
    #[prost(message, repeated, tag = "2")]
    pub outcome: ::prost::alloc::vec::Vec<crate::codec::pbcodec::ExecutionOutcomeWithId>,
    #[prost(message, repeated, tag = "3")]
    pub receipt: ::prost::alloc::vec::Vec<crate::codec::pbcodec::Receipt>,
}
