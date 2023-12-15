#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BigInt {
    #[prost(bytes = "vec", tag = "1")]
    pub bytes: ::prost::alloc::vec::Vec<u8>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Block {
    /// Firehose block version (unrelated to Arweave block version)
    #[prost(uint32, tag = "1")]
    pub ver: u32,
    /// The block identifier
    #[prost(bytes = "vec", tag = "2")]
    pub indep_hash: ::prost::alloc::vec::Vec<u8>,
    /// The nonce chosen to solve the mining problem
    #[prost(bytes = "vec", tag = "3")]
    pub nonce: ::prost::alloc::vec::Vec<u8>,
    /// `indep_hash` of the previous block in the weave
    #[prost(bytes = "vec", tag = "4")]
    pub previous_block: ::prost::alloc::vec::Vec<u8>,
    /// POSIX time of block discovery
    #[prost(uint64, tag = "5")]
    pub timestamp: u64,
    /// POSIX time of the last difficulty retarget
    #[prost(uint64, tag = "6")]
    pub last_retarget: u64,
    /// Mining difficulty; the number `hash` must be greater than.
    #[prost(message, optional, tag = "7")]
    pub diff: ::core::option::Option<BigInt>,
    /// How many blocks have passed since the genesis block
    #[prost(uint64, tag = "8")]
    pub height: u64,
    /// Mining solution hash of the block; must satisfy the mining difficulty
    #[prost(bytes = "vec", tag = "9")]
    pub hash: ::prost::alloc::vec::Vec<u8>,
    /// Merkle root of the tree of Merkle roots of block's transactions' data.
    #[prost(bytes = "vec", tag = "10")]
    pub tx_root: ::prost::alloc::vec::Vec<u8>,
    /// Transactions contained within this block
    #[prost(message, repeated, tag = "11")]
    pub txs: ::prost::alloc::vec::Vec<Transaction>,
    /// The root hash of the Merkle Patricia Tree containing
    /// all wallet (account) balances and the identifiers
    /// of the last transactions posted by them; if any.
    #[prost(bytes = "vec", tag = "12")]
    pub wallet_list: ::prost::alloc::vec::Vec<u8>,
    /// (string or) Address of the account to receive the block rewards. Can also be unclaimed which is encoded as a null byte
    #[prost(bytes = "vec", tag = "13")]
    pub reward_addr: ::prost::alloc::vec::Vec<u8>,
    /// Tags that a block producer can add to a block
    #[prost(message, repeated, tag = "14")]
    pub tags: ::prost::alloc::vec::Vec<Tag>,
    /// Size of reward pool
    #[prost(message, optional, tag = "15")]
    pub reward_pool: ::core::option::Option<BigInt>,
    /// Size of the weave in bytes
    #[prost(message, optional, tag = "16")]
    pub weave_size: ::core::option::Option<BigInt>,
    /// Size of this block in bytes
    #[prost(message, optional, tag = "17")]
    pub block_size: ::core::option::Option<BigInt>,
    /// Required after the version 1.8 fork. Zero otherwise.
    /// The sum of the average number of hashes computed
    /// by the network to produce the past blocks including this one.
    #[prost(message, optional, tag = "18")]
    pub cumulative_diff: ::core::option::Option<BigInt>,
    /// Required after the version 1.8 fork. Null byte otherwise.
    /// The Merkle root of the block index - the list of {`indep_hash`; `weave_size`; `tx_root`} triplets
    #[prost(bytes = "vec", tag = "20")]
    pub hash_list_merkle: ::prost::alloc::vec::Vec<u8>,
    /// The proof of access; Used after v2.4 only; set as defaults otherwise
    #[prost(message, optional, tag = "21")]
    pub poa: ::core::option::Option<ProofOfAccess>,
}
/// A succinct proof of access to a recall byte found in a TX
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ProofOfAccess {
    /// The recall byte option chosen; global offset of index byte
    #[prost(string, tag = "1")]
    pub option: ::prost::alloc::string::String,
    /// The path through the Merkle tree of transactions' `data_root`s;
    /// from the `data_root` being proven to the corresponding `tx_root`
    #[prost(bytes = "vec", tag = "2")]
    pub tx_path: ::prost::alloc::vec::Vec<u8>,
    /// The path through the Merkle tree of identifiers of chunks of the
    /// corresponding transaction; from the chunk being proven to the
    /// corresponding `data_root`.
    #[prost(bytes = "vec", tag = "3")]
    pub data_path: ::prost::alloc::vec::Vec<u8>,
    /// The data chunk.
    #[prost(bytes = "vec", tag = "4")]
    pub chunk: ::prost::alloc::vec::Vec<u8>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Transaction {
    /// 1 or 2 for v1 or v2 transactions. More allowable in the future
    #[prost(uint32, tag = "1")]
    pub format: u32,
    /// The transaction identifier.
    #[prost(bytes = "vec", tag = "2")]
    pub id: ::prost::alloc::vec::Vec<u8>,
    /// Either the identifier of the previous transaction from the same
    /// wallet or the identifier of one of the last ?MAX_TX_ANCHOR_DEPTH blocks.
    #[prost(bytes = "vec", tag = "3")]
    pub last_tx: ::prost::alloc::vec::Vec<u8>,
    /// The public key the transaction is signed with.
    #[prost(bytes = "vec", tag = "4")]
    pub owner: ::prost::alloc::vec::Vec<u8>,
    /// A list of arbitrary key-value pairs
    #[prost(message, repeated, tag = "5")]
    pub tags: ::prost::alloc::vec::Vec<Tag>,
    /// The address of the recipient; if any. The SHA2-256 hash of the public key.
    #[prost(bytes = "vec", tag = "6")]
    pub target: ::prost::alloc::vec::Vec<u8>,
    /// The amount of Winstons to send to the recipient; if any.
    #[prost(message, optional, tag = "7")]
    pub quantity: ::core::option::Option<BigInt>,
    /// The data to upload; if any. For v2 transactions; the field is optional
    /// - a fee is charged based on the `data_size` field;
    ///    data may be uploaded any time later in chunks.
    #[prost(bytes = "vec", tag = "8")]
    pub data: ::prost::alloc::vec::Vec<u8>,
    /// Size in bytes of the transaction data.
    #[prost(message, optional, tag = "9")]
    pub data_size: ::core::option::Option<BigInt>,
    /// The Merkle root of the Merkle tree of data chunks.
    #[prost(bytes = "vec", tag = "10")]
    pub data_root: ::prost::alloc::vec::Vec<u8>,
    /// The signature.
    #[prost(bytes = "vec", tag = "11")]
    pub signature: ::prost::alloc::vec::Vec<u8>,
    /// The fee in Winstons.
    #[prost(message, optional, tag = "12")]
    pub reward: ::core::option::Option<BigInt>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Tag {
    #[prost(bytes = "vec", tag = "1")]
    pub name: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub value: ::prost::alloc::vec::Vec<u8>,
}
