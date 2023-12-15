#[graph_runtime_derive::generate_asc_type(
    __required__{header:Header,
    result_begin_block:ResponseBeginBlock,
    result_end_block:ResponseEndBlock}
)]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type(
    __required__{header:Header,
    result_begin_block:ResponseBeginBlock,
    result_end_block:ResponseEndBlock}
)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Block {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<Header>,
    #[prost(message, optional, tag = "2")]
    pub evidence: ::core::option::Option<EvidenceList>,
    #[prost(message, optional, tag = "3")]
    pub last_commit: ::core::option::Option<Commit>,
    #[prost(message, optional, tag = "4")]
    pub result_begin_block: ::core::option::Option<ResponseBeginBlock>,
    #[prost(message, optional, tag = "5")]
    pub result_end_block: ::core::option::Option<ResponseEndBlock>,
    #[prost(message, repeated, tag = "7")]
    pub transactions: ::prost::alloc::vec::Vec<TxResult>,
    #[prost(message, repeated, tag = "8")]
    pub validator_updates: ::prost::alloc::vec::Vec<Validator>,
}
/// HeaderOnlyBlock is a standard \[Block\] structure where all other fields are
/// removed so that hydrating that object from a \[Block\] bytes payload will
/// drastically reduce allocated memory required to hold the full block.
///
/// This can be used to unpack a \[Block\] when only the \[Header\] information
/// is required and greatly reduce required memory.
#[graph_runtime_derive::generate_asc_type(__required__{header:Header})]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type(__required__{header:Header})]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HeaderOnlyBlock {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<Header>,
}
#[graph_runtime_derive::generate_asc_type(
    __required__{event:Event,
    block:HeaderOnlyBlock}
)]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type(
    __required__{event:Event,
    block:HeaderOnlyBlock}
)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EventData {
    #[prost(message, optional, tag = "1")]
    pub event: ::core::option::Option<Event>,
    #[prost(message, optional, tag = "2")]
    pub block: ::core::option::Option<HeaderOnlyBlock>,
    #[prost(message, optional, tag = "3")]
    pub tx: ::core::option::Option<TransactionContext>,
}
#[graph_runtime_derive::generate_asc_type(
    __required__{tx:TxResult,
    block:HeaderOnlyBlock}
)]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type(
    __required__{tx:TxResult,
    block:HeaderOnlyBlock}
)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TransactionData {
    #[prost(message, optional, tag = "1")]
    pub tx: ::core::option::Option<TxResult>,
    #[prost(message, optional, tag = "2")]
    pub block: ::core::option::Option<HeaderOnlyBlock>,
}
#[graph_runtime_derive::generate_asc_type(
    __required__{message:Any,
    block:HeaderOnlyBlock,
    tx:TransactionContext}
)]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type(
    __required__{message:Any,
    block:HeaderOnlyBlock,
    tx:TransactionContext}
)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MessageData {
    #[prost(message, optional, tag = "1")]
    pub message: ::core::option::Option<::prost_types::Any>,
    #[prost(message, optional, tag = "2")]
    pub block: ::core::option::Option<HeaderOnlyBlock>,
    #[prost(message, optional, tag = "3")]
    pub tx: ::core::option::Option<TransactionContext>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TransactionContext {
    #[prost(bytes = "vec", tag = "1")]
    pub hash: ::prost::alloc::vec::Vec<u8>,
    #[prost(uint32, tag = "2")]
    pub index: u32,
    #[prost(uint32, tag = "3")]
    pub code: u32,
    #[prost(int64, tag = "4")]
    pub gas_wanted: i64,
    #[prost(int64, tag = "5")]
    pub gas_used: i64,
}
#[graph_runtime_derive::generate_asc_type(__required__{last_block_id:BlockID})]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type(__required__{last_block_id:BlockID})]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Header {
    #[prost(message, optional, tag = "1")]
    pub version: ::core::option::Option<Consensus>,
    #[prost(string, tag = "2")]
    pub chain_id: ::prost::alloc::string::String,
    #[prost(uint64, tag = "3")]
    pub height: u64,
    #[prost(message, optional, tag = "4")]
    pub time: ::core::option::Option<Timestamp>,
    #[prost(message, optional, tag = "5")]
    pub last_block_id: ::core::option::Option<BlockId>,
    #[prost(bytes = "vec", tag = "6")]
    pub last_commit_hash: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "7")]
    pub data_hash: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "8")]
    pub validators_hash: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "9")]
    pub next_validators_hash: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "10")]
    pub consensus_hash: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "11")]
    pub app_hash: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "12")]
    pub last_results_hash: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "13")]
    pub evidence_hash: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "14")]
    pub proposer_address: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "15")]
    pub hash: ::prost::alloc::vec::Vec<u8>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Consensus {
    #[prost(uint64, tag = "1")]
    pub block: u64,
    #[prost(uint64, tag = "2")]
    pub app: u64,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Timestamp {
    #[prost(int64, tag = "1")]
    pub seconds: i64,
    #[prost(int32, tag = "2")]
    pub nanos: i32,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BlockId {
    #[prost(bytes = "vec", tag = "1")]
    pub hash: ::prost::alloc::vec::Vec<u8>,
    #[prost(message, optional, tag = "2")]
    pub part_set_header: ::core::option::Option<PartSetHeader>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PartSetHeader {
    #[prost(uint32, tag = "1")]
    pub total: u32,
    #[prost(bytes = "vec", tag = "2")]
    pub hash: ::prost::alloc::vec::Vec<u8>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EvidenceList {
    #[prost(message, repeated, tag = "1")]
    pub evidence: ::prost::alloc::vec::Vec<Evidence>,
}
#[graph_runtime_derive::generate_asc_type(
    sum{duplicate_vote_evidence:DuplicateVoteEvidence,
    light_client_attack_evidence:LightClientAttackEvidence}
)]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type(
    sum{duplicate_vote_evidence:DuplicateVoteEvidence,
    light_client_attack_evidence:LightClientAttackEvidence}
)]
#[graph_runtime_derive::generate_array_type(Cosmos)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Evidence {
    #[prost(oneof = "evidence::Sum", tags = "1, 2")]
    pub sum: ::core::option::Option<evidence::Sum>,
}
/// Nested message and enum types in `Evidence`.
pub mod evidence {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Sum {
        #[prost(message, tag = "1")]
        DuplicateVoteEvidence(super::DuplicateVoteEvidence),
        #[prost(message, tag = "2")]
        LightClientAttackEvidence(super::LightClientAttackEvidence),
    }
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DuplicateVoteEvidence {
    #[prost(message, optional, tag = "1")]
    pub vote_a: ::core::option::Option<EventVote>,
    #[prost(message, optional, tag = "2")]
    pub vote_b: ::core::option::Option<EventVote>,
    #[prost(int64, tag = "3")]
    pub total_voting_power: i64,
    #[prost(int64, tag = "4")]
    pub validator_power: i64,
    #[prost(message, optional, tag = "5")]
    pub timestamp: ::core::option::Option<Timestamp>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EventVote {
    #[prost(enumeration = "SignedMsgType", tag = "1")]
    pub event_vote_type: i32,
    #[prost(uint64, tag = "2")]
    pub height: u64,
    #[prost(int32, tag = "3")]
    pub round: i32,
    #[prost(message, optional, tag = "4")]
    pub block_id: ::core::option::Option<BlockId>,
    #[prost(message, optional, tag = "5")]
    pub timestamp: ::core::option::Option<Timestamp>,
    #[prost(bytes = "vec", tag = "6")]
    pub validator_address: ::prost::alloc::vec::Vec<u8>,
    #[prost(int32, tag = "7")]
    pub validator_index: i32,
    #[prost(bytes = "vec", tag = "8")]
    pub signature: ::prost::alloc::vec::Vec<u8>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LightClientAttackEvidence {
    #[prost(message, optional, tag = "1")]
    pub conflicting_block: ::core::option::Option<LightBlock>,
    #[prost(int64, tag = "2")]
    pub common_height: i64,
    #[prost(message, repeated, tag = "3")]
    pub byzantine_validators: ::prost::alloc::vec::Vec<Validator>,
    #[prost(int64, tag = "4")]
    pub total_voting_power: i64,
    #[prost(message, optional, tag = "5")]
    pub timestamp: ::core::option::Option<Timestamp>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LightBlock {
    #[prost(message, optional, tag = "1")]
    pub signed_header: ::core::option::Option<SignedHeader>,
    #[prost(message, optional, tag = "2")]
    pub validator_set: ::core::option::Option<ValidatorSet>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SignedHeader {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<Header>,
    #[prost(message, optional, tag = "2")]
    pub commit: ::core::option::Option<Commit>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Commit {
    #[prost(int64, tag = "1")]
    pub height: i64,
    #[prost(int32, tag = "2")]
    pub round: i32,
    #[prost(message, optional, tag = "3")]
    pub block_id: ::core::option::Option<BlockId>,
    #[prost(message, repeated, tag = "4")]
    pub signatures: ::prost::alloc::vec::Vec<CommitSig>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[graph_runtime_derive::generate_array_type(Cosmos)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CommitSig {
    #[prost(enumeration = "BlockIdFlag", tag = "1")]
    pub block_id_flag: i32,
    #[prost(bytes = "vec", tag = "2")]
    pub validator_address: ::prost::alloc::vec::Vec<u8>,
    #[prost(message, optional, tag = "3")]
    pub timestamp: ::core::option::Option<Timestamp>,
    #[prost(bytes = "vec", tag = "4")]
    pub signature: ::prost::alloc::vec::Vec<u8>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ValidatorSet {
    #[prost(message, repeated, tag = "1")]
    pub validators: ::prost::alloc::vec::Vec<Validator>,
    #[prost(message, optional, tag = "2")]
    pub proposer: ::core::option::Option<Validator>,
    #[prost(int64, tag = "3")]
    pub total_voting_power: i64,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[graph_runtime_derive::generate_array_type(Cosmos)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Validator {
    #[prost(bytes = "vec", tag = "1")]
    pub address: ::prost::alloc::vec::Vec<u8>,
    #[prost(message, optional, tag = "2")]
    pub pub_key: ::core::option::Option<PublicKey>,
    #[prost(int64, tag = "3")]
    pub voting_power: i64,
    #[prost(int64, tag = "4")]
    pub proposer_priority: i64,
}
#[graph_runtime_derive::generate_asc_type(sum{ed25519:Vec<u8>, secp256k1:Vec<u8>})]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type(sum{ed25519:Vec<u8>, secp256k1:Vec<u8>})]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PublicKey {
    #[prost(oneof = "public_key::Sum", tags = "1, 2")]
    pub sum: ::core::option::Option<public_key::Sum>,
}
/// Nested message and enum types in `PublicKey`.
pub mod public_key {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Sum {
        #[prost(bytes, tag = "1")]
        Ed25519(::prost::alloc::vec::Vec<u8>),
        #[prost(bytes, tag = "2")]
        Secp256k1(::prost::alloc::vec::Vec<u8>),
    }
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ResponseBeginBlock {
    #[prost(message, repeated, tag = "1")]
    pub events: ::prost::alloc::vec::Vec<Event>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[graph_runtime_derive::generate_array_type(Cosmos)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Event {
    #[prost(string, tag = "1")]
    pub event_type: ::prost::alloc::string::String,
    #[prost(message, repeated, tag = "2")]
    pub attributes: ::prost::alloc::vec::Vec<EventAttribute>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[graph_runtime_derive::generate_array_type(Cosmos)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EventAttribute {
    #[prost(string, tag = "1")]
    pub key: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub value: ::prost::alloc::string::String,
    #[prost(bool, tag = "3")]
    pub index: bool,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ResponseEndBlock {
    #[prost(message, repeated, tag = "1")]
    pub validator_updates: ::prost::alloc::vec::Vec<ValidatorUpdate>,
    #[prost(message, optional, tag = "2")]
    pub consensus_param_updates: ::core::option::Option<ConsensusParams>,
    #[prost(message, repeated, tag = "3")]
    pub events: ::prost::alloc::vec::Vec<Event>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[graph_runtime_derive::generate_array_type(Cosmos)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ValidatorUpdate {
    #[prost(bytes = "vec", tag = "1")]
    pub address: ::prost::alloc::vec::Vec<u8>,
    #[prost(message, optional, tag = "2")]
    pub pub_key: ::core::option::Option<PublicKey>,
    #[prost(int64, tag = "3")]
    pub power: i64,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ConsensusParams {
    #[prost(message, optional, tag = "1")]
    pub block: ::core::option::Option<BlockParams>,
    #[prost(message, optional, tag = "2")]
    pub evidence: ::core::option::Option<EvidenceParams>,
    #[prost(message, optional, tag = "3")]
    pub validator: ::core::option::Option<ValidatorParams>,
    #[prost(message, optional, tag = "4")]
    pub version: ::core::option::Option<VersionParams>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BlockParams {
    #[prost(int64, tag = "1")]
    pub max_bytes: i64,
    #[prost(int64, tag = "2")]
    pub max_gas: i64,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EvidenceParams {
    #[prost(int64, tag = "1")]
    pub max_age_num_blocks: i64,
    #[prost(message, optional, tag = "2")]
    pub max_age_duration: ::core::option::Option<Duration>,
    #[prost(int64, tag = "3")]
    pub max_bytes: i64,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Duration {
    #[prost(int64, tag = "1")]
    pub seconds: i64,
    #[prost(int32, tag = "2")]
    pub nanos: i32,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ValidatorParams {
    #[prost(string, repeated, tag = "1")]
    pub pub_key_types: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct VersionParams {
    #[prost(uint64, tag = "1")]
    pub app_version: u64,
}
#[graph_runtime_derive::generate_asc_type(__required__{tx:Tx, result:ResponseDeliverTx})]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type(
    __required__{tx:Tx,
    result:ResponseDeliverTx}
)]
#[graph_runtime_derive::generate_array_type(Cosmos)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TxResult {
    #[prost(uint64, tag = "1")]
    pub height: u64,
    #[prost(uint32, tag = "2")]
    pub index: u32,
    #[prost(message, optional, tag = "3")]
    pub tx: ::core::option::Option<Tx>,
    #[prost(message, optional, tag = "4")]
    pub result: ::core::option::Option<ResponseDeliverTx>,
    #[prost(bytes = "vec", tag = "5")]
    pub hash: ::prost::alloc::vec::Vec<u8>,
}
#[graph_runtime_derive::generate_asc_type(__required__{body:TxBody})]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type(__required__{body:TxBody})]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Tx {
    #[prost(message, optional, tag = "1")]
    pub body: ::core::option::Option<TxBody>,
    #[prost(message, optional, tag = "2")]
    pub auth_info: ::core::option::Option<AuthInfo>,
    #[prost(bytes = "vec", repeated, tag = "3")]
    pub signatures: ::prost::alloc::vec::Vec<::prost::alloc::vec::Vec<u8>>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TxBody {
    #[prost(message, repeated, tag = "1")]
    pub messages: ::prost::alloc::vec::Vec<::prost_types::Any>,
    #[prost(string, tag = "2")]
    pub memo: ::prost::alloc::string::String,
    #[prost(uint64, tag = "3")]
    pub timeout_height: u64,
    #[prost(message, repeated, tag = "1023")]
    pub extension_options: ::prost::alloc::vec::Vec<::prost_types::Any>,
    #[prost(message, repeated, tag = "2047")]
    pub non_critical_extension_options: ::prost::alloc::vec::Vec<::prost_types::Any>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[graph_runtime_derive::generate_array_type(Cosmos)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Any {
    #[prost(string, tag = "1")]
    pub type_url: ::prost::alloc::string::String,
    #[prost(bytes = "vec", tag = "2")]
    pub value: ::prost::alloc::vec::Vec<u8>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AuthInfo {
    #[prost(message, repeated, tag = "1")]
    pub signer_infos: ::prost::alloc::vec::Vec<SignerInfo>,
    #[prost(message, optional, tag = "2")]
    pub fee: ::core::option::Option<Fee>,
    #[prost(message, optional, tag = "3")]
    pub tip: ::core::option::Option<Tip>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[graph_runtime_derive::generate_array_type(Cosmos)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SignerInfo {
    #[prost(message, optional, tag = "1")]
    pub public_key: ::core::option::Option<::prost_types::Any>,
    #[prost(message, optional, tag = "2")]
    pub mode_info: ::core::option::Option<ModeInfo>,
    #[prost(uint64, tag = "3")]
    pub sequence: u64,
}
#[graph_runtime_derive::generate_asc_type(
    sum{single:ModeInfoSingle,
    multi:ModeInfoMulti}
)]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type(
    sum{single:ModeInfoSingle,
    multi:ModeInfoMulti}
)]
#[graph_runtime_derive::generate_array_type(Cosmos)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ModeInfo {
    #[prost(oneof = "mode_info::Sum", tags = "1, 2")]
    pub sum: ::core::option::Option<mode_info::Sum>,
}
/// Nested message and enum types in `ModeInfo`.
pub mod mode_info {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Sum {
        #[prost(message, tag = "1")]
        Single(super::ModeInfoSingle),
        #[prost(message, tag = "2")]
        Multi(super::ModeInfoMulti),
    }
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ModeInfoSingle {
    #[prost(enumeration = "SignMode", tag = "1")]
    pub mode: i32,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ModeInfoMulti {
    #[prost(message, optional, tag = "1")]
    pub bitarray: ::core::option::Option<CompactBitArray>,
    #[prost(message, repeated, tag = "2")]
    pub mode_infos: ::prost::alloc::vec::Vec<ModeInfo>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CompactBitArray {
    #[prost(uint32, tag = "1")]
    pub extra_bits_stored: u32,
    #[prost(bytes = "vec", tag = "2")]
    pub elems: ::prost::alloc::vec::Vec<u8>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Fee {
    #[prost(message, repeated, tag = "1")]
    pub amount: ::prost::alloc::vec::Vec<Coin>,
    #[prost(uint64, tag = "2")]
    pub gas_limit: u64,
    #[prost(string, tag = "3")]
    pub payer: ::prost::alloc::string::String,
    #[prost(string, tag = "4")]
    pub granter: ::prost::alloc::string::String,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[graph_runtime_derive::generate_array_type(Cosmos)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Coin {
    #[prost(string, tag = "1")]
    pub denom: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub amount: ::prost::alloc::string::String,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Tip {
    #[prost(message, repeated, tag = "1")]
    pub amount: ::prost::alloc::vec::Vec<Coin>,
    #[prost(string, tag = "2")]
    pub tipper: ::prost::alloc::string::String,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ResponseDeliverTx {
    #[prost(uint32, tag = "1")]
    pub code: u32,
    #[prost(bytes = "vec", tag = "2")]
    pub data: ::prost::alloc::vec::Vec<u8>,
    #[prost(string, tag = "3")]
    pub log: ::prost::alloc::string::String,
    #[prost(string, tag = "4")]
    pub info: ::prost::alloc::string::String,
    #[prost(int64, tag = "5")]
    pub gas_wanted: i64,
    #[prost(int64, tag = "6")]
    pub gas_used: i64,
    #[prost(message, repeated, tag = "7")]
    pub events: ::prost::alloc::vec::Vec<Event>,
    #[prost(string, tag = "8")]
    pub codespace: ::prost::alloc::string::String,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Cosmos)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ValidatorSetUpdates {
    #[prost(message, repeated, tag = "1")]
    pub validator_updates: ::prost::alloc::vec::Vec<Validator>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum SignedMsgType {
    Unknown = 0,
    Prevote = 1,
    Precommit = 2,
    Proposal = 32,
}
impl SignedMsgType {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            SignedMsgType::Unknown => "SIGNED_MSG_TYPE_UNKNOWN",
            SignedMsgType::Prevote => "SIGNED_MSG_TYPE_PREVOTE",
            SignedMsgType::Precommit => "SIGNED_MSG_TYPE_PRECOMMIT",
            SignedMsgType::Proposal => "SIGNED_MSG_TYPE_PROPOSAL",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "SIGNED_MSG_TYPE_UNKNOWN" => Some(Self::Unknown),
            "SIGNED_MSG_TYPE_PREVOTE" => Some(Self::Prevote),
            "SIGNED_MSG_TYPE_PRECOMMIT" => Some(Self::Precommit),
            "SIGNED_MSG_TYPE_PROPOSAL" => Some(Self::Proposal),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum BlockIdFlag {
    Unknown = 0,
    Absent = 1,
    Commit = 2,
    Nil = 3,
}
impl BlockIdFlag {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            BlockIdFlag::Unknown => "BLOCK_ID_FLAG_UNKNOWN",
            BlockIdFlag::Absent => "BLOCK_ID_FLAG_ABSENT",
            BlockIdFlag::Commit => "BLOCK_ID_FLAG_COMMIT",
            BlockIdFlag::Nil => "BLOCK_ID_FLAG_NIL",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "BLOCK_ID_FLAG_UNKNOWN" => Some(Self::Unknown),
            "BLOCK_ID_FLAG_ABSENT" => Some(Self::Absent),
            "BLOCK_ID_FLAG_COMMIT" => Some(Self::Commit),
            "BLOCK_ID_FLAG_NIL" => Some(Self::Nil),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum SignMode {
    Unspecified = 0,
    Direct = 1,
    Textual = 2,
    LegacyAminoJson = 127,
}
impl SignMode {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            SignMode::Unspecified => "SIGN_MODE_UNSPECIFIED",
            SignMode::Direct => "SIGN_MODE_DIRECT",
            SignMode::Textual => "SIGN_MODE_TEXTUAL",
            SignMode::LegacyAminoJson => "SIGN_MODE_LEGACY_AMINO_JSON",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "SIGN_MODE_UNSPECIFIED" => Some(Self::Unspecified),
            "SIGN_MODE_DIRECT" => Some(Self::Direct),
            "SIGN_MODE_TEXTUAL" => Some(Self::Textual),
            "SIGN_MODE_LEGACY_AMINO_JSON" => Some(Self::LegacyAminoJson),
            _ => None,
        }
    }
}
