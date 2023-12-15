/// CombinedFilter is a combination of "LogFilters" and "CallToFilters"
///
/// It transforms the requested stream in two ways:
///    1. STRIPPING
///       The block data is stripped from all transactions that don't
///       match any of the filters.
///
///    2. SKIPPING
///       If an "block index" covers a range containing a
///       block that does NOT match any of the filters, the block will be
///       skipped altogether, UNLESS send_all_block_headers is enabled
///       In that case, the block would still be sent, but without any
///       transactionTrace
///
/// The SKIPPING feature only applies to historical blocks, because
/// the "block index" is always produced after the merged-blocks files
/// are produced. Therefore, the "live" blocks are never filtered out.
///
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CombinedFilter {
    #[prost(message, repeated, tag = "1")]
    pub log_filters: ::prost::alloc::vec::Vec<LogFilter>,
    #[prost(message, repeated, tag = "2")]
    pub call_filters: ::prost::alloc::vec::Vec<CallToFilter>,
    /// Always send all blocks. if they don't match any log_filters or call_filters,
    /// all the transactions will be filtered out, sending only the header.
    #[prost(bool, tag = "3")]
    pub send_all_block_headers: bool,
}
/// MultiLogFilter concatenates the results of each LogFilter (inclusive OR)
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MultiLogFilter {
    #[prost(message, repeated, tag = "1")]
    pub log_filters: ::prost::alloc::vec::Vec<LogFilter>,
}
/// LogFilter will match calls where *BOTH*
/// * the contract address that emits the log is one in the provided addresses -- OR addresses list is empty --
/// * the event signature (topic.0) is one of the provided event_signatures -- OR event_signatures is empty --
///
/// a LogFilter with both empty addresses and event_signatures lists is invalid and will fail.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LogFilter {
    #[prost(bytes = "vec", repeated, tag = "1")]
    pub addresses: ::prost::alloc::vec::Vec<::prost::alloc::vec::Vec<u8>>,
    /// corresponds to the keccak of the event signature which is stores in topic.0
    #[prost(bytes = "vec", repeated, tag = "2")]
    pub event_signatures: ::prost::alloc::vec::Vec<::prost::alloc::vec::Vec<u8>>,
}
/// MultiCallToFilter concatenates the results of each CallToFilter (inclusive OR)
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MultiCallToFilter {
    #[prost(message, repeated, tag = "1")]
    pub call_filters: ::prost::alloc::vec::Vec<CallToFilter>,
}
/// CallToFilter will match calls where *BOTH*
/// * the contract address (TO) is one in the provided addresses -- OR addresses list is empty --
/// * the method signature (in 4-bytes format) is one of the provided signatures -- OR signatures is empty --
///
/// a CallToFilter with both empty addresses and signatures lists is invalid and will fail.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CallToFilter {
    #[prost(bytes = "vec", repeated, tag = "1")]
    pub addresses: ::prost::alloc::vec::Vec<::prost::alloc::vec::Vec<u8>>,
    #[prost(bytes = "vec", repeated, tag = "2")]
    pub signatures: ::prost::alloc::vec::Vec<::prost::alloc::vec::Vec<u8>>,
}
/// Deprecated: LightBlock is deprecated, replaced by HeaderOnly, note however that the new transform
/// does not have any transactions traces returned, so it's not a direct replacement.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LightBlock {}
/// HeaderOnly returns only the block's header and few top-level core information for the block. Useful
/// for cases where no transactions information is required at all.
///
/// The structure that would will have access to after:
///
/// ```ignore
/// Block {
///   int32 ver = 1;
///   bytes hash = 2;
///   uint64 number = 3;
///   uint64 size = 4;
///   BlockHeader header = 5;
/// }
/// ```
///
/// Everything else will be empty.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HeaderOnly {}
