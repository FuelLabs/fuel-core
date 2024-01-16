#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Fuel)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Block {
    #[prost(bytes = "vec", tag = "1")]
    pub id: ::prost::alloc::vec::Vec<u8>,
    #[prost(uint32, tag = "2")]
    pub height: u32,
    #[prost(uint64, tag = "3")]
    pub da_height: u64,
    #[prost(uint64, tag = "4")]
    pub msg_receipt_count: u64,
    #[prost(bytes = "vec", tag = "5")]
    pub tx_root: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "6")]
    pub msg_receipt_root: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "7")]
    pub prev_id: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "8")]
    pub prev_root: ::prost::alloc::vec::Vec<u8>,
    #[prost(fixed64, tag = "9")]
    pub timestamp: u64,
    #[prost(bytes = "vec", tag = "10")]
    pub application_hash: ::prost::alloc::vec::Vec<u8>,
    #[prost(message, repeated, tag = "11")]
    pub transactions: ::prost::alloc::vec::Vec<Transaction>,
}
#[graph_runtime_derive::generate_asc_type(kind{script:Script, create:Create, mint:Mint})]
#[graph_runtime_derive::generate_network_type_id(Fuel)]
#[graph_runtime_derive::generate_from_rust_type(
    kind{script:Script,
    create:Create,
    mint:Mint}
)]
#[graph_runtime_derive::generate_array_type(Fuel)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Transaction {
    #[prost(oneof = "transaction::Kind", tags = "1, 2, 3")]
    pub kind: ::core::option::Option<transaction::Kind>,
}
/// Nested message and enum types in `Transaction`.
pub mod transaction {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Kind {
        #[prost(message, tag = "1")]
        Script(super::Script),
        #[prost(message, tag = "2")]
        Create(super::Create),
        #[prost(message, tag = "3")]
        Mint(super::Mint),
    }
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Fuel)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Script {
    #[prost(uint64, tag = "1")]
    pub script_gas_limit: u64,
    #[prost(bytes = "vec", tag = "2")]
    pub script: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    pub script_data: ::prost::alloc::vec::Vec<u8>,
    #[prost(message, optional, tag = "4")]
    pub policies: ::core::option::Option<Policies>,
    #[prost(message, repeated, tag = "5")]
    pub inputs: ::prost::alloc::vec::Vec<Input>,
    #[prost(message, repeated, tag = "6")]
    pub outputs: ::prost::alloc::vec::Vec<Output>,
    #[prost(bytes = "vec", repeated, tag = "7")]
    pub witnesses: ::prost::alloc::vec::Vec<::prost::alloc::vec::Vec<u8>>,
    #[prost(bytes = "vec", tag = "8")]
    pub receipts_root: ::prost::alloc::vec::Vec<u8>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Fuel)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Create {
    #[prost(uint64, tag = "1")]
    pub bytecode_length: u64,
    #[prost(uint32, tag = "2")]
    pub bytecode_witness_index: u32,
    #[prost(message, optional, tag = "3")]
    pub policies: ::core::option::Option<Policies>,
    #[prost(message, repeated, tag = "4")]
    pub storage_slots: ::prost::alloc::vec::Vec<StorageSlot>,
    #[prost(message, repeated, tag = "5")]
    pub inputs: ::prost::alloc::vec::Vec<Input>,
    #[prost(message, repeated, tag = "6")]
    pub outputs: ::prost::alloc::vec::Vec<Output>,
    #[prost(bytes = "vec", repeated, tag = "7")]
    pub witnesses: ::prost::alloc::vec::Vec<::prost::alloc::vec::Vec<u8>>,
    #[prost(bytes = "vec", tag = "8")]
    pub salt: ::prost::alloc::vec::Vec<u8>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Fuel)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Mint {
    #[prost(message, optional, tag = "1")]
    pub tx_pointer: ::core::option::Option<TxPointer>,
    #[prost(message, optional, tag = "2")]
    pub input_contract: ::core::option::Option<InputContract>,
    #[prost(message, optional, tag = "3")]
    pub output_contract: ::core::option::Option<OutputContract>,
    #[prost(uint64, tag = "4")]
    pub mint_amount: u64,
    #[prost(bytes = "vec", tag = "5")]
    pub mint_asset_id: ::prost::alloc::vec::Vec<u8>,
}
#[graph_runtime_derive::generate_asc_type(
    kind{coin_signed:Coin,
    coin_predicate:Coin,
    contract:InputContract,
    message_coin_signed:Message,
    message_coin_predicate:Message,
    message_data_signed:Message,
    message_data_predicate:Message}
)]
#[graph_runtime_derive::generate_network_type_id(Fuel)]
#[graph_runtime_derive::generate_from_rust_type(
    kind{coin_signed:Coin,
    coin_predicate:Coin,
    contract:InputContract,
    message_coin_signed:Message,
    message_coin_predicate:Message,
    message_data_signed:Message,
    message_data_predicate:Message}
)]
#[graph_runtime_derive::generate_array_type(Fuel)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Input {
    #[prost(oneof = "input::Kind", tags = "1, 2, 3, 4, 5, 6, 7")]
    pub kind: ::core::option::Option<input::Kind>,
}
/// Nested message and enum types in `Input`.
pub mod input {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Kind {
        #[prost(message, tag = "1")]
        CoinSigned(super::Coin),
        #[prost(message, tag = "2")]
        CoinPredicate(super::Coin),
        #[prost(message, tag = "3")]
        Contract(super::InputContract),
        #[prost(message, tag = "4")]
        MessageCoinSigned(super::Message),
        #[prost(message, tag = "5")]
        MessageCoinPredicate(super::Message),
        #[prost(message, tag = "6")]
        MessageDataSigned(super::Message),
        #[prost(message, tag = "7")]
        MessageDataPredicate(super::Message),
    }
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Fuel)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Coin {
    #[prost(message, optional, tag = "1")]
    pub utxo_id: ::core::option::Option<UtxoId>,
    #[prost(bytes = "vec", tag = "2")]
    pub owner: ::prost::alloc::vec::Vec<u8>,
    #[prost(uint64, tag = "3")]
    pub amount: u64,
    #[prost(bytes = "vec", tag = "4")]
    pub asset_id: ::prost::alloc::vec::Vec<u8>,
    #[prost(message, optional, tag = "5")]
    pub tx_pointer: ::core::option::Option<TxPointer>,
    #[prost(uint32, tag = "6")]
    pub witness_index: u32,
    #[prost(uint32, tag = "7")]
    pub maturity: u32,
    #[prost(uint64, tag = "8")]
    pub predicate_gas_used: u64,
    #[prost(bytes = "vec", tag = "9")]
    pub predicate: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "10")]
    pub predicate_data: ::prost::alloc::vec::Vec<u8>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Fuel)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Message {
    #[prost(bytes = "vec", tag = "1")]
    pub sender: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub recipient: ::prost::alloc::vec::Vec<u8>,
    #[prost(uint64, tag = "3")]
    pub amount: u64,
    #[prost(bytes = "vec", tag = "4")]
    pub nonce: ::prost::alloc::vec::Vec<u8>,
    #[prost(uint32, tag = "5")]
    pub witness_index: u32,
    #[prost(uint64, tag = "6")]
    pub predicate_gas_used: u64,
    #[prost(bytes = "vec", tag = "7")]
    pub data: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "8")]
    pub predicate: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "9")]
    pub predicate_data: ::prost::alloc::vec::Vec<u8>,
}
#[graph_runtime_derive::generate_asc_type(
    kind{coin:OutputCoin,
    contract:OutputContract,
    change:OutputCoin,
    variable:OutputCoin,
    contract_created:OutputContractCreated}
)]
#[graph_runtime_derive::generate_network_type_id(Fuel)]
#[graph_runtime_derive::generate_from_rust_type(
    kind{coin:OutputCoin,
    contract:OutputContract,
    change:OutputCoin,
    variable:OutputCoin,
    contract_created:OutputContractCreated}
)]
#[graph_runtime_derive::generate_array_type(Fuel)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Output {
    #[prost(oneof = "output::Kind", tags = "1, 2, 3, 4, 5")]
    pub kind: ::core::option::Option<output::Kind>,
}
/// Nested message and enum types in `Output`.
pub mod output {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Kind {
        #[prost(message, tag = "1")]
        Coin(super::OutputCoin),
        #[prost(message, tag = "2")]
        Contract(super::OutputContract),
        #[prost(message, tag = "3")]
        Change(super::OutputCoin),
        #[prost(message, tag = "4")]
        Variable(super::OutputCoin),
        #[prost(message, tag = "5")]
        ContractCreated(super::OutputContractCreated),
    }
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Fuel)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OutputCoin {
    #[prost(bytes = "vec", tag = "1")]
    pub to: ::prost::alloc::vec::Vec<u8>,
    #[prost(uint64, tag = "2")]
    pub amount: u64,
    #[prost(bytes = "vec", tag = "3")]
    pub asset_id: ::prost::alloc::vec::Vec<u8>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Fuel)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OutputContractCreated {
    #[prost(bytes = "vec", tag = "1")]
    pub contract_id: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub state_root: ::prost::alloc::vec::Vec<u8>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Fuel)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct InputContract {
    #[prost(message, optional, tag = "1")]
    pub utxo_id: ::core::option::Option<UtxoId>,
    #[prost(bytes = "vec", tag = "2")]
    pub balance_root: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    pub state_root: ::prost::alloc::vec::Vec<u8>,
    #[prost(message, optional, tag = "4")]
    pub tx_pointer: ::core::option::Option<TxPointer>,
    #[prost(bytes = "vec", tag = "5")]
    pub contract_id: ::prost::alloc::vec::Vec<u8>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Fuel)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OutputContract {
    #[prost(uint32, tag = "1")]
    pub input_index: u32,
    #[prost(bytes = "vec", tag = "2")]
    pub balance_root: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    pub state_root: ::prost::alloc::vec::Vec<u8>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Fuel)]
#[graph_runtime_derive::generate_from_rust_type()]
#[graph_runtime_derive::generate_array_type(Fuel)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StorageSlot {
    #[prost(bytes = "vec", tag = "1")]
    pub key: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub value: ::prost::alloc::vec::Vec<u8>,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Fuel)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UtxoId {
    #[prost(bytes = "vec", tag = "1")]
    pub tx_id: ::prost::alloc::vec::Vec<u8>,
    #[prost(uint32, tag = "2")]
    pub output_index: u32,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Fuel)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TxPointer {
    #[prost(uint32, tag = "1")]
    pub block_height: u32,
    #[prost(uint32, tag = "2")]
    pub tx_index: u32,
}
#[graph_runtime_derive::generate_asc_type()]
#[graph_runtime_derive::generate_network_type_id(Fuel)]
#[graph_runtime_derive::generate_from_rust_type()]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Policies {
    #[prost(uint64, repeated, tag = "1")]
    pub values: ::prost::alloc::vec::Vec<u64>,
}
