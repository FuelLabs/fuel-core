// @generated
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Block {
    #[prost(string, tag="1")]
    pub author: ::prost::alloc::string::String,
    #[prost(message, optional, tag="2")]
    pub header: ::core::option::Option<BlockHeader>,
    #[prost(message, repeated, tag="3")]
    pub chunk_headers: ::prost::alloc::vec::Vec<ChunkHeader>,
    #[prost(message, repeated, tag="4")]
    pub shards: ::prost::alloc::vec::Vec<IndexerShard>,
    #[prost(message, repeated, tag="5")]
    pub state_changes: ::prost::alloc::vec::Vec<StateChangeWithCause>,
}
/// HeaderOnlyBlock is a standard \[Block\] structure where all other fields are
/// removed so that hydrating that object from a \[Block\] bytes payload will
/// drastically reduced allocated memory required to hold the full block.
///
/// This can be used to unpack a \[Block\] when only the \[BlockHeader\] information
/// is required and greatly reduced required memory.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HeaderOnlyBlock {
    #[prost(message, optional, tag="2")]
    pub header: ::core::option::Option<BlockHeader>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StateChangeWithCause {
    #[prost(message, optional, tag="1")]
    pub value: ::core::option::Option<StateChangeValue>,
    #[prost(message, optional, tag="2")]
    pub cause: ::core::option::Option<StateChangeCause>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StateChangeCause {
    #[prost(oneof="state_change_cause::Cause", tags="1, 2, 3, 4, 5, 6, 7, 8, 9, 10")]
    pub cause: ::core::option::Option<state_change_cause::Cause>,
}
/// Nested message and enum types in `StateChangeCause`.
pub mod state_change_cause {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct NotWritableToDisk {
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct InitialState {
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct TransactionProcessing {
        #[prost(message, optional, tag="1")]
        pub tx_hash: ::core::option::Option<super::CryptoHash>,
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct ActionReceiptProcessingStarted {
        #[prost(message, optional, tag="1")]
        pub receipt_hash: ::core::option::Option<super::CryptoHash>,
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct ActionReceiptGasReward {
        #[prost(message, optional, tag="1")]
        pub tx_hash: ::core::option::Option<super::CryptoHash>,
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct ReceiptProcessing {
        #[prost(message, optional, tag="1")]
        pub tx_hash: ::core::option::Option<super::CryptoHash>,
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct PostponedReceipt {
        #[prost(message, optional, tag="1")]
        pub tx_hash: ::core::option::Option<super::CryptoHash>,
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct UpdatedDelayedReceipts {
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct ValidatorAccountsUpdate {
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Migration {
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Cause {
        #[prost(message, tag="1")]
        NotWritableToDisk(NotWritableToDisk),
        #[prost(message, tag="2")]
        InitialState(InitialState),
        #[prost(message, tag="3")]
        TransactionProcessing(TransactionProcessing),
        #[prost(message, tag="4")]
        ActionReceiptProcessingStarted(ActionReceiptProcessingStarted),
        #[prost(message, tag="5")]
        ActionReceiptGasReward(ActionReceiptGasReward),
        #[prost(message, tag="6")]
        ReceiptProcessing(ReceiptProcessing),
        #[prost(message, tag="7")]
        PostponedReceipt(PostponedReceipt),
        #[prost(message, tag="8")]
        UpdatedDelayedReceipts(UpdatedDelayedReceipts),
        #[prost(message, tag="9")]
        ValidatorAccountsUpdate(ValidatorAccountsUpdate),
        #[prost(message, tag="10")]
        Migration(Migration),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StateChangeValue {
    #[prost(oneof="state_change_value::Value", tags="1, 2, 3, 4, 5, 6, 7, 8")]
    pub value: ::core::option::Option<state_change_value::Value>,
}
/// Nested message and enum types in `StateChangeValue`.
pub mod state_change_value {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct AccountUpdate {
        #[prost(string, tag="1")]
        pub account_id: ::prost::alloc::string::String,
        #[prost(message, optional, tag="2")]
        pub account: ::core::option::Option<super::Account>,
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct AccountDeletion {
        #[prost(string, tag="1")]
        pub account_id: ::prost::alloc::string::String,
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct AccessKeyUpdate {
        #[prost(string, tag="1")]
        pub account_id: ::prost::alloc::string::String,
        #[prost(message, optional, tag="2")]
        pub public_key: ::core::option::Option<super::PublicKey>,
        #[prost(message, optional, tag="3")]
        pub access_key: ::core::option::Option<super::AccessKey>,
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct AccessKeyDeletion {
        #[prost(string, tag="1")]
        pub account_id: ::prost::alloc::string::String,
        #[prost(message, optional, tag="2")]
        pub public_key: ::core::option::Option<super::PublicKey>,
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct DataUpdate {
        #[prost(string, tag="1")]
        pub account_id: ::prost::alloc::string::String,
        #[prost(bytes="vec", tag="2")]
        pub key: ::prost::alloc::vec::Vec<u8>,
        #[prost(bytes="vec", tag="3")]
        pub value: ::prost::alloc::vec::Vec<u8>,
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct DataDeletion {
        #[prost(string, tag="1")]
        pub account_id: ::prost::alloc::string::String,
        #[prost(bytes="vec", tag="2")]
        pub key: ::prost::alloc::vec::Vec<u8>,
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct ContractCodeUpdate {
        #[prost(string, tag="1")]
        pub account_id: ::prost::alloc::string::String,
        #[prost(bytes="vec", tag="2")]
        pub code: ::prost::alloc::vec::Vec<u8>,
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct ContractCodeDeletion {
        #[prost(string, tag="1")]
        pub account_id: ::prost::alloc::string::String,
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Value {
        #[prost(message, tag="1")]
        AccountUpdate(AccountUpdate),
        #[prost(message, tag="2")]
        AccountDeletion(AccountDeletion),
        #[prost(message, tag="3")]
        AccessKeyUpdate(AccessKeyUpdate),
        #[prost(message, tag="4")]
        AccessKeyDeletion(AccessKeyDeletion),
        #[prost(message, tag="5")]
        DataUpdate(DataUpdate),
        #[prost(message, tag="6")]
        DataDeletion(DataDeletion),
        #[prost(message, tag="7")]
        ContractCodeUpdate(ContractCodeUpdate),
        #[prost(message, tag="8")]
        ContractDeletion(ContractCodeDeletion),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Account {
    #[prost(message, optional, tag="1")]
    pub amount: ::core::option::Option<BigInt>,
    #[prost(message, optional, tag="2")]
    pub locked: ::core::option::Option<BigInt>,
    #[prost(message, optional, tag="3")]
    pub code_hash: ::core::option::Option<CryptoHash>,
    #[prost(uint64, tag="4")]
    pub storage_usage: u64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BlockHeader {
    #[prost(uint64, tag="1")]
    pub height: u64,
    #[prost(uint64, tag="2")]
    pub prev_height: u64,
    #[prost(message, optional, tag="3")]
    pub epoch_id: ::core::option::Option<CryptoHash>,
    #[prost(message, optional, tag="4")]
    pub next_epoch_id: ::core::option::Option<CryptoHash>,
    #[prost(message, optional, tag="5")]
    pub hash: ::core::option::Option<CryptoHash>,
    #[prost(message, optional, tag="6")]
    pub prev_hash: ::core::option::Option<CryptoHash>,
    #[prost(message, optional, tag="7")]
    pub prev_state_root: ::core::option::Option<CryptoHash>,
    #[prost(message, optional, tag="8")]
    pub chunk_receipts_root: ::core::option::Option<CryptoHash>,
    #[prost(message, optional, tag="9")]
    pub chunk_headers_root: ::core::option::Option<CryptoHash>,
    #[prost(message, optional, tag="10")]
    pub chunk_tx_root: ::core::option::Option<CryptoHash>,
    #[prost(message, optional, tag="11")]
    pub outcome_root: ::core::option::Option<CryptoHash>,
    #[prost(uint64, tag="12")]
    pub chunks_included: u64,
    #[prost(message, optional, tag="13")]
    pub challenges_root: ::core::option::Option<CryptoHash>,
    #[prost(uint64, tag="14")]
    pub timestamp: u64,
    #[prost(uint64, tag="15")]
    pub timestamp_nanosec: u64,
    #[prost(message, optional, tag="16")]
    pub random_value: ::core::option::Option<CryptoHash>,
    #[prost(message, repeated, tag="17")]
    pub validator_proposals: ::prost::alloc::vec::Vec<ValidatorStake>,
    #[prost(bool, repeated, tag="18")]
    pub chunk_mask: ::prost::alloc::vec::Vec<bool>,
    #[prost(message, optional, tag="19")]
    pub gas_price: ::core::option::Option<BigInt>,
    #[prost(uint64, tag="20")]
    pub block_ordinal: u64,
    #[prost(message, optional, tag="21")]
    pub total_supply: ::core::option::Option<BigInt>,
    #[prost(message, repeated, tag="22")]
    pub challenges_result: ::prost::alloc::vec::Vec<SlashedValidator>,
    #[prost(uint64, tag="23")]
    pub last_final_block_height: u64,
    #[prost(message, optional, tag="24")]
    pub last_final_block: ::core::option::Option<CryptoHash>,
    #[prost(uint64, tag="25")]
    pub last_ds_final_block_height: u64,
    #[prost(message, optional, tag="26")]
    pub last_ds_final_block: ::core::option::Option<CryptoHash>,
    #[prost(message, optional, tag="27")]
    pub next_bp_hash: ::core::option::Option<CryptoHash>,
    #[prost(message, optional, tag="28")]
    pub block_merkle_root: ::core::option::Option<CryptoHash>,
    #[prost(bytes="vec", tag="29")]
    pub epoch_sync_data_hash: ::prost::alloc::vec::Vec<u8>,
    #[prost(message, repeated, tag="30")]
    pub approvals: ::prost::alloc::vec::Vec<Signature>,
    #[prost(message, optional, tag="31")]
    pub signature: ::core::option::Option<Signature>,
    #[prost(uint32, tag="32")]
    pub latest_protocol_version: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BigInt {
    #[prost(bytes="vec", tag="1")]
    pub bytes: ::prost::alloc::vec::Vec<u8>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CryptoHash {
    #[prost(bytes="vec", tag="1")]
    pub bytes: ::prost::alloc::vec::Vec<u8>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Signature {
    #[prost(enumeration="CurveKind", tag="1")]
    pub r#type: i32,
    #[prost(bytes="vec", tag="2")]
    pub bytes: ::prost::alloc::vec::Vec<u8>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PublicKey {
    #[prost(enumeration="CurveKind", tag="1")]
    pub r#type: i32,
    #[prost(bytes="vec", tag="2")]
    pub bytes: ::prost::alloc::vec::Vec<u8>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ValidatorStake {
    #[prost(string, tag="1")]
    pub account_id: ::prost::alloc::string::String,
    #[prost(message, optional, tag="2")]
    pub public_key: ::core::option::Option<PublicKey>,
    #[prost(message, optional, tag="3")]
    pub stake: ::core::option::Option<BigInt>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SlashedValidator {
    #[prost(string, tag="1")]
    pub account_id: ::prost::alloc::string::String,
    #[prost(bool, tag="2")]
    pub is_double_sign: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ChunkHeader {
    #[prost(bytes="vec", tag="1")]
    pub chunk_hash: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes="vec", tag="2")]
    pub prev_block_hash: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes="vec", tag="3")]
    pub outcome_root: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes="vec", tag="4")]
    pub prev_state_root: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes="vec", tag="5")]
    pub encoded_merkle_root: ::prost::alloc::vec::Vec<u8>,
    #[prost(uint64, tag="6")]
    pub encoded_length: u64,
    #[prost(uint64, tag="7")]
    pub height_created: u64,
    #[prost(uint64, tag="8")]
    pub height_included: u64,
    #[prost(uint64, tag="9")]
    pub shard_id: u64,
    #[prost(uint64, tag="10")]
    pub gas_used: u64,
    #[prost(uint64, tag="11")]
    pub gas_limit: u64,
    #[prost(message, optional, tag="12")]
    pub validator_reward: ::core::option::Option<BigInt>,
    #[prost(message, optional, tag="13")]
    pub balance_burnt: ::core::option::Option<BigInt>,
    #[prost(bytes="vec", tag="14")]
    pub outgoing_receipts_root: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes="vec", tag="15")]
    pub tx_root: ::prost::alloc::vec::Vec<u8>,
    #[prost(message, repeated, tag="16")]
    pub validator_proposals: ::prost::alloc::vec::Vec<ValidatorStake>,
    #[prost(message, optional, tag="17")]
    pub signature: ::core::option::Option<Signature>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct IndexerShard {
    #[prost(uint64, tag="1")]
    pub shard_id: u64,
    #[prost(message, optional, tag="2")]
    pub chunk: ::core::option::Option<IndexerChunk>,
    #[prost(message, repeated, tag="3")]
    pub receipt_execution_outcomes: ::prost::alloc::vec::Vec<IndexerExecutionOutcomeWithReceipt>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct IndexerExecutionOutcomeWithReceipt {
    #[prost(message, optional, tag="1")]
    pub execution_outcome: ::core::option::Option<ExecutionOutcomeWithId>,
    #[prost(message, optional, tag="2")]
    pub receipt: ::core::option::Option<Receipt>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct IndexerChunk {
    #[prost(string, tag="1")]
    pub author: ::prost::alloc::string::String,
    #[prost(message, optional, tag="2")]
    pub header: ::core::option::Option<ChunkHeader>,
    #[prost(message, repeated, tag="3")]
    pub transactions: ::prost::alloc::vec::Vec<IndexerTransactionWithOutcome>,
    #[prost(message, repeated, tag="4")]
    pub receipts: ::prost::alloc::vec::Vec<Receipt>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct IndexerTransactionWithOutcome {
    #[prost(message, optional, tag="1")]
    pub transaction: ::core::option::Option<SignedTransaction>,
    #[prost(message, optional, tag="2")]
    pub outcome: ::core::option::Option<IndexerExecutionOutcomeWithOptionalReceipt>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SignedTransaction {
    #[prost(string, tag="1")]
    pub signer_id: ::prost::alloc::string::String,
    #[prost(message, optional, tag="2")]
    pub public_key: ::core::option::Option<PublicKey>,
    #[prost(uint64, tag="3")]
    pub nonce: u64,
    #[prost(string, tag="4")]
    pub receiver_id: ::prost::alloc::string::String,
    #[prost(message, repeated, tag="5")]
    pub actions: ::prost::alloc::vec::Vec<Action>,
    #[prost(message, optional, tag="6")]
    pub signature: ::core::option::Option<Signature>,
    #[prost(message, optional, tag="7")]
    pub hash: ::core::option::Option<CryptoHash>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct IndexerExecutionOutcomeWithOptionalReceipt {
    #[prost(message, optional, tag="1")]
    pub execution_outcome: ::core::option::Option<ExecutionOutcomeWithId>,
    #[prost(message, optional, tag="2")]
    pub receipt: ::core::option::Option<Receipt>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Receipt {
    #[prost(string, tag="1")]
    pub predecessor_id: ::prost::alloc::string::String,
    #[prost(string, tag="2")]
    pub receiver_id: ::prost::alloc::string::String,
    #[prost(message, optional, tag="3")]
    pub receipt_id: ::core::option::Option<CryptoHash>,
    #[prost(oneof="receipt::Receipt", tags="10, 11")]
    pub receipt: ::core::option::Option<receipt::Receipt>,
}
/// Nested message and enum types in `Receipt`.
pub mod receipt {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Receipt {
        #[prost(message, tag="10")]
        Action(super::ReceiptAction),
        #[prost(message, tag="11")]
        Data(super::ReceiptData),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ReceiptData {
    #[prost(message, optional, tag="1")]
    pub data_id: ::core::option::Option<CryptoHash>,
    #[prost(bytes="vec", tag="2")]
    pub data: ::prost::alloc::vec::Vec<u8>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ReceiptAction {
    #[prost(string, tag="1")]
    pub signer_id: ::prost::alloc::string::String,
    #[prost(message, optional, tag="2")]
    pub signer_public_key: ::core::option::Option<PublicKey>,
    #[prost(message, optional, tag="3")]
    pub gas_price: ::core::option::Option<BigInt>,
    #[prost(message, repeated, tag="4")]
    pub output_data_receivers: ::prost::alloc::vec::Vec<DataReceiver>,
    #[prost(message, repeated, tag="5")]
    pub input_data_ids: ::prost::alloc::vec::Vec<CryptoHash>,
    #[prost(message, repeated, tag="6")]
    pub actions: ::prost::alloc::vec::Vec<Action>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DataReceiver {
    #[prost(message, optional, tag="1")]
    pub data_id: ::core::option::Option<CryptoHash>,
    #[prost(string, tag="2")]
    pub receiver_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ExecutionOutcomeWithId {
    #[prost(message, optional, tag="1")]
    pub proof: ::core::option::Option<MerklePath>,
    #[prost(message, optional, tag="2")]
    pub block_hash: ::core::option::Option<CryptoHash>,
    #[prost(message, optional, tag="3")]
    pub id: ::core::option::Option<CryptoHash>,
    #[prost(message, optional, tag="4")]
    pub outcome: ::core::option::Option<ExecutionOutcome>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ExecutionOutcome {
    #[prost(string, repeated, tag="1")]
    pub logs: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(message, repeated, tag="2")]
    pub receipt_ids: ::prost::alloc::vec::Vec<CryptoHash>,
    #[prost(uint64, tag="3")]
    pub gas_burnt: u64,
    #[prost(message, optional, tag="4")]
    pub tokens_burnt: ::core::option::Option<BigInt>,
    #[prost(string, tag="5")]
    pub executor_id: ::prost::alloc::string::String,
    #[prost(enumeration="ExecutionMetadata", tag="6")]
    pub metadata: i32,
    #[prost(oneof="execution_outcome::Status", tags="20, 21, 22, 23")]
    pub status: ::core::option::Option<execution_outcome::Status>,
}
/// Nested message and enum types in `ExecutionOutcome`.
pub mod execution_outcome {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Status {
        #[prost(message, tag="20")]
        Unknown(super::UnknownExecutionStatus),
        #[prost(message, tag="21")]
        Failure(super::FailureExecutionStatus),
        #[prost(message, tag="22")]
        SuccessValue(super::SuccessValueExecutionStatus),
        #[prost(message, tag="23")]
        SuccessReceiptId(super::SuccessReceiptIdExecutionStatus),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SuccessValueExecutionStatus {
    #[prost(bytes="vec", tag="1")]
    pub value: ::prost::alloc::vec::Vec<u8>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SuccessReceiptIdExecutionStatus {
    #[prost(message, optional, tag="1")]
    pub id: ::core::option::Option<CryptoHash>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UnknownExecutionStatus {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FailureExecutionStatus {
    #[prost(oneof="failure_execution_status::Failure", tags="1, 2")]
    pub failure: ::core::option::Option<failure_execution_status::Failure>,
}
/// Nested message and enum types in `FailureExecutionStatus`.
pub mod failure_execution_status {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Failure {
        #[prost(message, tag="1")]
        ActionError(super::ActionError),
        #[prost(enumeration="super::InvalidTxError", tag="2")]
        InvalidTxError(i32),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ActionError {
    #[prost(uint64, tag="1")]
    pub index: u64,
    #[prost(oneof="action_error::Kind", tags="21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42")]
    pub kind: ::core::option::Option<action_error::Kind>,
}
/// Nested message and enum types in `ActionError`.
pub mod action_error {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Kind {
        #[prost(message, tag="21")]
        AccountAlreadyExist(super::AccountAlreadyExistsErrorKind),
        #[prost(message, tag="22")]
        AccountDoesNotExist(super::AccountDoesNotExistErrorKind),
        #[prost(message, tag="23")]
        CreateAccountOnlyByRegistrar(super::CreateAccountOnlyByRegistrarErrorKind),
        #[prost(message, tag="24")]
        CreateAccountNotAllowed(super::CreateAccountNotAllowedErrorKind),
        #[prost(message, tag="25")]
        ActorNoPermission(super::ActorNoPermissionErrorKind),
        #[prost(message, tag="26")]
        DeleteKeyDoesNotExist(super::DeleteKeyDoesNotExistErrorKind),
        #[prost(message, tag="27")]
        AddKeyAlreadyExists(super::AddKeyAlreadyExistsErrorKind),
        #[prost(message, tag="28")]
        DeleteAccountStaking(super::DeleteAccountStakingErrorKind),
        #[prost(message, tag="29")]
        LackBalanceForState(super::LackBalanceForStateErrorKind),
        #[prost(message, tag="30")]
        TriesToUnstake(super::TriesToUnstakeErrorKind),
        #[prost(message, tag="31")]
        TriesToStake(super::TriesToStakeErrorKind),
        #[prost(message, tag="32")]
        InsufficientStake(super::InsufficientStakeErrorKind),
        #[prost(message, tag="33")]
        FunctionCall(super::FunctionCallErrorKind),
        #[prost(message, tag="34")]
        NewReceiptValidation(super::NewReceiptValidationErrorKind),
        #[prost(message, tag="35")]
        OnlyImplicitAccountCreationAllowed(super::OnlyImplicitAccountCreationAllowedErrorKind),
        #[prost(message, tag="36")]
        DeleteAccountWithLargeState(super::DeleteAccountWithLargeStateErrorKind),
        #[prost(message, tag="37")]
        DelegateActionInvalidSignature(super::DelegateActionInvalidSignatureKind),
        #[prost(message, tag="38")]
        DelegateActionSenderDoesNotMatchTxReceiver(super::DelegateActionSenderDoesNotMatchTxReceiverKind),
        #[prost(message, tag="39")]
        DelegateActionExpired(super::DelegateActionExpiredKind),
        #[prost(message, tag="40")]
        DelegateActionAccessKeyError(super::DelegateActionAccessKeyErrorKind),
        #[prost(message, tag="41")]
        DelegateActionInvalidNonce(super::DelegateActionInvalidNonceKind),
        #[prost(message, tag="42")]
        DelegateActionNonceTooLarge(super::DelegateActionNonceTooLargeKind),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AccountAlreadyExistsErrorKind {
    #[prost(string, tag="1")]
    pub account_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AccountDoesNotExistErrorKind {
    #[prost(string, tag="1")]
    pub account_id: ::prost::alloc::string::String,
}
/// / A top-level account ID can only be created by registrar.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CreateAccountOnlyByRegistrarErrorKind {
    #[prost(string, tag="1")]
    pub account_id: ::prost::alloc::string::String,
    #[prost(string, tag="2")]
    pub registrar_account_id: ::prost::alloc::string::String,
    #[prost(string, tag="3")]
    pub predecessor_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CreateAccountNotAllowedErrorKind {
    #[prost(string, tag="1")]
    pub account_id: ::prost::alloc::string::String,
    #[prost(string, tag="2")]
    pub predecessor_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ActorNoPermissionErrorKind {
    #[prost(string, tag="1")]
    pub account_id: ::prost::alloc::string::String,
    #[prost(string, tag="2")]
    pub actor_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DeleteKeyDoesNotExistErrorKind {
    #[prost(string, tag="1")]
    pub account_id: ::prost::alloc::string::String,
    #[prost(message, optional, tag="2")]
    pub public_key: ::core::option::Option<PublicKey>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AddKeyAlreadyExistsErrorKind {
    #[prost(string, tag="1")]
    pub account_id: ::prost::alloc::string::String,
    #[prost(message, optional, tag="2")]
    pub public_key: ::core::option::Option<PublicKey>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DeleteAccountStakingErrorKind {
    #[prost(string, tag="1")]
    pub account_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LackBalanceForStateErrorKind {
    #[prost(string, tag="1")]
    pub account_id: ::prost::alloc::string::String,
    #[prost(message, optional, tag="2")]
    pub balance: ::core::option::Option<BigInt>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TriesToUnstakeErrorKind {
    #[prost(string, tag="1")]
    pub account_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TriesToStakeErrorKind {
    #[prost(string, tag="1")]
    pub account_id: ::prost::alloc::string::String,
    #[prost(message, optional, tag="2")]
    pub stake: ::core::option::Option<BigInt>,
    #[prost(message, optional, tag="3")]
    pub locked: ::core::option::Option<BigInt>,
    #[prost(message, optional, tag="4")]
    pub balance: ::core::option::Option<BigInt>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct InsufficientStakeErrorKind {
    #[prost(string, tag="1")]
    pub account_id: ::prost::alloc::string::String,
    #[prost(message, optional, tag="2")]
    pub stake: ::core::option::Option<BigInt>,
    #[prost(message, optional, tag="3")]
    pub minimum_stake: ::core::option::Option<BigInt>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FunctionCallErrorKind {
    #[prost(enumeration="FunctionCallErrorSer", tag="1")]
    pub error: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewReceiptValidationErrorKind {
    #[prost(enumeration="ReceiptValidationError", tag="1")]
    pub error: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OnlyImplicitAccountCreationAllowedErrorKind {
    #[prost(string, tag="1")]
    pub account_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DeleteAccountWithLargeStateErrorKind {
    #[prost(string, tag="1")]
    pub account_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DelegateActionInvalidSignatureKind {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DelegateActionSenderDoesNotMatchTxReceiverKind {
    #[prost(string, tag="1")]
    pub sender_id: ::prost::alloc::string::String,
    #[prost(string, tag="2")]
    pub receiver_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DelegateActionExpiredKind {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DelegateActionAccessKeyErrorKind {
    /// InvalidAccessKeyError
    #[prost(enumeration="InvalidTxError", tag="1")]
    pub error: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DelegateActionInvalidNonceKind {
    #[prost(uint64, tag="1")]
    pub delegate_nonce: u64,
    #[prost(uint64, tag="2")]
    pub ak_nonce: u64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DelegateActionNonceTooLargeKind {
    #[prost(uint64, tag="1")]
    pub delegate_nonce: u64,
    #[prost(uint64, tag="2")]
    pub upper_bound: u64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MerklePath {
    #[prost(message, repeated, tag="1")]
    pub path: ::prost::alloc::vec::Vec<MerklePathItem>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MerklePathItem {
    #[prost(message, optional, tag="1")]
    pub hash: ::core::option::Option<CryptoHash>,
    #[prost(enumeration="Direction", tag="2")]
    pub direction: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Action {
    #[prost(oneof="action::Action", tags="1, 2, 3, 4, 5, 6, 7, 8, 9")]
    pub action: ::core::option::Option<action::Action>,
}
/// Nested message and enum types in `Action`.
pub mod action {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Action {
        #[prost(message, tag="1")]
        CreateAccount(super::CreateAccountAction),
        #[prost(message, tag="2")]
        DeployContract(super::DeployContractAction),
        #[prost(message, tag="3")]
        FunctionCall(super::FunctionCallAction),
        #[prost(message, tag="4")]
        Transfer(super::TransferAction),
        #[prost(message, tag="5")]
        Stake(super::StakeAction),
        #[prost(message, tag="6")]
        AddKey(super::AddKeyAction),
        #[prost(message, tag="7")]
        DeleteKey(super::DeleteKeyAction),
        #[prost(message, tag="8")]
        DeleteAccount(super::DeleteAccountAction),
        #[prost(message, tag="9")]
        Delegate(super::SignedDelegateAction),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CreateAccountAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DeployContractAction {
    #[prost(bytes="vec", tag="1")]
    pub code: ::prost::alloc::vec::Vec<u8>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FunctionCallAction {
    #[prost(string, tag="1")]
    pub method_name: ::prost::alloc::string::String,
    #[prost(bytes="vec", tag="2")]
    pub args: ::prost::alloc::vec::Vec<u8>,
    #[prost(uint64, tag="3")]
    pub gas: u64,
    #[prost(message, optional, tag="4")]
    pub deposit: ::core::option::Option<BigInt>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TransferAction {
    #[prost(message, optional, tag="1")]
    pub deposit: ::core::option::Option<BigInt>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StakeAction {
    #[prost(message, optional, tag="1")]
    pub stake: ::core::option::Option<BigInt>,
    #[prost(message, optional, tag="2")]
    pub public_key: ::core::option::Option<PublicKey>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AddKeyAction {
    #[prost(message, optional, tag="1")]
    pub public_key: ::core::option::Option<PublicKey>,
    #[prost(message, optional, tag="2")]
    pub access_key: ::core::option::Option<AccessKey>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DeleteKeyAction {
    #[prost(message, optional, tag="1")]
    pub public_key: ::core::option::Option<PublicKey>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DeleteAccountAction {
    #[prost(string, tag="1")]
    pub beneficiary_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SignedDelegateAction {
    #[prost(message, optional, tag="1")]
    pub signature: ::core::option::Option<Signature>,
    #[prost(message, optional, tag="2")]
    pub delegate_action: ::core::option::Option<DelegateAction>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DelegateAction {
    #[prost(string, tag="1")]
    pub sender_id: ::prost::alloc::string::String,
    #[prost(string, tag="2")]
    pub receiver_id: ::prost::alloc::string::String,
    #[prost(message, repeated, tag="3")]
    pub actions: ::prost::alloc::vec::Vec<Action>,
    #[prost(uint64, tag="4")]
    pub nonce: u64,
    #[prost(uint64, tag="5")]
    pub max_block_height: u64,
    #[prost(message, optional, tag="6")]
    pub public_key: ::core::option::Option<PublicKey>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AccessKey {
    #[prost(uint64, tag="1")]
    pub nonce: u64,
    #[prost(message, optional, tag="2")]
    pub permission: ::core::option::Option<AccessKeyPermission>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AccessKeyPermission {
    #[prost(oneof="access_key_permission::Permission", tags="1, 2")]
    pub permission: ::core::option::Option<access_key_permission::Permission>,
}
/// Nested message and enum types in `AccessKeyPermission`.
pub mod access_key_permission {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Permission {
        #[prost(message, tag="1")]
        FunctionCall(super::FunctionCallPermission),
        #[prost(message, tag="2")]
        FullAccess(super::FullAccessPermission),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FunctionCallPermission {
    #[prost(message, optional, tag="1")]
    pub allowance: ::core::option::Option<BigInt>,
    #[prost(string, tag="2")]
    pub receiver_id: ::prost::alloc::string::String,
    #[prost(string, repeated, tag="3")]
    pub method_names: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FullAccessPermission {
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum CurveKind {
    Ed25519 = 0,
    Secp256k1 = 1,
}
impl CurveKind {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            CurveKind::Ed25519 => "ED25519",
            CurveKind::Secp256k1 => "SECP256K1",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "ED25519" => Some(Self::Ed25519),
            "SECP256K1" => Some(Self::Secp256k1),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum ExecutionMetadata {
    V1 = 0,
}
impl ExecutionMetadata {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            ExecutionMetadata::V1 => "ExecutionMetadataV1",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "ExecutionMetadataV1" => Some(Self::V1),
            _ => None,
        }
    }
}
/// todo: add more detail?
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum FunctionCallErrorSer {
    CompilationError = 0,
    LinkError = 1,
    MethodResolveError = 2,
    WasmTrap = 3,
    WasmUnknownError = 4,
    HostError = 5,
    EvmError = 6,
    ExecutionError = 7,
}
impl FunctionCallErrorSer {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            FunctionCallErrorSer::CompilationError => "CompilationError",
            FunctionCallErrorSer::LinkError => "LinkError",
            FunctionCallErrorSer::MethodResolveError => "MethodResolveError",
            FunctionCallErrorSer::WasmTrap => "WasmTrap",
            FunctionCallErrorSer::WasmUnknownError => "WasmUnknownError",
            FunctionCallErrorSer::HostError => "HostError",
            FunctionCallErrorSer::EvmError => "_EVMError",
            FunctionCallErrorSer::ExecutionError => "ExecutionError",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "CompilationError" => Some(Self::CompilationError),
            "LinkError" => Some(Self::LinkError),
            "MethodResolveError" => Some(Self::MethodResolveError),
            "WasmTrap" => Some(Self::WasmTrap),
            "WasmUnknownError" => Some(Self::WasmUnknownError),
            "HostError" => Some(Self::HostError),
            "_EVMError" => Some(Self::EvmError),
            "ExecutionError" => Some(Self::ExecutionError),
            _ => None,
        }
    }
}
/// todo: add more detail?
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum ReceiptValidationError {
    InvalidPredecessorId = 0,
    InvalidReceiverAccountId = 1,
    InvalidSignerAccountId = 2,
    InvalidDataReceiverId = 3,
    ReturnedValueLengthExceeded = 4,
    NumberInputDataDependenciesExceeded = 5,
    ActionsValidationError = 6,
}
impl ReceiptValidationError {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            ReceiptValidationError::InvalidPredecessorId => "InvalidPredecessorId",
            ReceiptValidationError::InvalidReceiverAccountId => "InvalidReceiverAccountId",
            ReceiptValidationError::InvalidSignerAccountId => "InvalidSignerAccountId",
            ReceiptValidationError::InvalidDataReceiverId => "InvalidDataReceiverId",
            ReceiptValidationError::ReturnedValueLengthExceeded => "ReturnedValueLengthExceeded",
            ReceiptValidationError::NumberInputDataDependenciesExceeded => "NumberInputDataDependenciesExceeded",
            ReceiptValidationError::ActionsValidationError => "ActionsValidationError",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "InvalidPredecessorId" => Some(Self::InvalidPredecessorId),
            "InvalidReceiverAccountId" => Some(Self::InvalidReceiverAccountId),
            "InvalidSignerAccountId" => Some(Self::InvalidSignerAccountId),
            "InvalidDataReceiverId" => Some(Self::InvalidDataReceiverId),
            "ReturnedValueLengthExceeded" => Some(Self::ReturnedValueLengthExceeded),
            "NumberInputDataDependenciesExceeded" => Some(Self::NumberInputDataDependenciesExceeded),
            "ActionsValidationError" => Some(Self::ActionsValidationError),
            _ => None,
        }
    }
}
/// todo: add more detail?
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum InvalidTxError {
    InvalidAccessKeyError = 0,
    InvalidSignerId = 1,
    SignerDoesNotExist = 2,
    InvalidNonce = 3,
    NonceTooLarge = 4,
    InvalidReceiverId = 5,
    InvalidSignature = 6,
    NotEnoughBalance = 7,
    LackBalanceForState = 8,
    CostOverflow = 9,
    InvalidChain = 10,
    Expired = 11,
    ActionsValidation = 12,
    TransactionSizeExceeded = 13,
}
impl InvalidTxError {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            InvalidTxError::InvalidAccessKeyError => "InvalidAccessKeyError",
            InvalidTxError::InvalidSignerId => "InvalidSignerId",
            InvalidTxError::SignerDoesNotExist => "SignerDoesNotExist",
            InvalidTxError::InvalidNonce => "InvalidNonce",
            InvalidTxError::NonceTooLarge => "NonceTooLarge",
            InvalidTxError::InvalidReceiverId => "InvalidReceiverId",
            InvalidTxError::InvalidSignature => "InvalidSignature",
            InvalidTxError::NotEnoughBalance => "NotEnoughBalance",
            InvalidTxError::LackBalanceForState => "LackBalanceForState",
            InvalidTxError::CostOverflow => "CostOverflow",
            InvalidTxError::InvalidChain => "InvalidChain",
            InvalidTxError::Expired => "Expired",
            InvalidTxError::ActionsValidation => "ActionsValidation",
            InvalidTxError::TransactionSizeExceeded => "TransactionSizeExceeded",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "InvalidAccessKeyError" => Some(Self::InvalidAccessKeyError),
            "InvalidSignerId" => Some(Self::InvalidSignerId),
            "SignerDoesNotExist" => Some(Self::SignerDoesNotExist),
            "InvalidNonce" => Some(Self::InvalidNonce),
            "NonceTooLarge" => Some(Self::NonceTooLarge),
            "InvalidReceiverId" => Some(Self::InvalidReceiverId),
            "InvalidSignature" => Some(Self::InvalidSignature),
            "NotEnoughBalance" => Some(Self::NotEnoughBalance),
            "LackBalanceForState" => Some(Self::LackBalanceForState),
            "CostOverflow" => Some(Self::CostOverflow),
            "InvalidChain" => Some(Self::InvalidChain),
            "Expired" => Some(Self::Expired),
            "ActionsValidation" => Some(Self::ActionsValidation),
            "TransactionSizeExceeded" => Some(Self::TransactionSizeExceeded),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum Direction {
    Left = 0,
    Right = 1,
}
impl Direction {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            Direction::Left => "left",
            Direction::Right => "right",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            _ => None,
        }
    }
}
// @@protoc_insertion_point(module)
