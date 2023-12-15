#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EventTypeFilter {
    #[prost(string, repeated, tag = "1")]
    pub event_types: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
