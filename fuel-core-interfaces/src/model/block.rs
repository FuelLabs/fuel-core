pub use super::BlockHeight;
use crate::{
    common::{
        fuel_crypto::Hasher,
        fuel_tx::{
            Bytes32,
            Transaction,
        },
        fuel_types::bytes::SerializableVec,
    },
    model::DaBlockHeight,
};
use chrono::{
    DateTime,
    Utc,
};
use fuel_vm::{
    fuel_crypto,
    fuel_merkle,
    fuel_types::MessageId,
    prelude::Signature,
};

use crate::common::{
    fuel_tx::Input,
    fuel_types::Address,
};
#[cfg(any(test, feature = "test-helpers"))]
use chrono::TimeZone;
use derive_more::{
    AsRef,
    Display,
    From,
    FromStr,
    Into,
    LowerHex,
    UpperHex,
};

/// A cryptographically secure hash, identifying a block.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    FromStr,
    From,
    Into,
    LowerHex,
    UpperHex,
    Display,
    AsRef,
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[repr(transparent)]
pub struct BlockId(Bytes32);

impl BlockId {
    /// Converts the hash into a message having the same bytes.
    pub fn into_message(self) -> fuel_crypto::Message {
        // This is safe because BlockId is a cryptographically secure hash.
        unsafe { fuel_crypto::Message::from_bytes_unchecked(*self.0) }
        // Without this, the signature would be using a hash of the id making it more
        // difficult to verify.
    }

    pub fn as_message(&self) -> &fuel_crypto::Message {
        // This is safe because BlockId is a cryptographically secure hash.
        unsafe { fuel_crypto::Message::as_ref_unchecked(self.0.as_slice()) }
        // Without this, the signature would be using a hash of the id making it more
        // difficult to verify.
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(any(test, feature = "test-helpers"), derive(Default))]
/// A fuel block header that has all the fields generated because it
/// has been executed.
pub struct FuelBlockHeader {
    /// The application header.
    pub application: FuelApplicationHeader<GeneratedApplicationFields>,
    /// The consensus header.
    pub consensus: FuelConsensusHeader<GeneratedConsensusFields>,
    /// Header Metadata
    #[cfg_attr(feature = "serde", serde(skip))]
    pub metadata: Option<HeaderMetadata>,
}

#[derive(Clone, Debug)]
#[cfg_attr(any(test, feature = "test-helpers"), derive(Default))]
/// A partially complete fuel block header that doesn't not
/// have any generated fields because it has not been executed yet.
pub struct PartialFuelBlockHeader {
    /// The application header.
    pub application: FuelApplicationHeader<Empty>,
    /// The consensus header.
    pub consensus: FuelConsensusHeader<Empty>,
    /// Header Metadata
    pub metadata: Option<HeaderMetadata>,
}

#[derive(Clone, Copy, Debug, Default)]
/// Empty generated fields.
pub struct Empty;

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(any(test, feature = "test-helpers"), derive(Default))]
/// The fuel block application header.
/// Contains everything except consensus related data.
pub struct FuelApplicationHeader<Generated> {
    /// The layer 1 height of messages and events to include since the last layer 1 block number.
    /// This is not meant to represent the layer 1 block this was committed to. Validators will need
    /// to have some rules in place to ensure the block number was chosen in a reasonable way. For
    /// example, they should verify that the block number satisfies the finality requirements of the
    /// layer 1 chain. They should also verify that the block number isn't too stale and is increasing.
    /// Some similar concerns are noted in this issue: https://github.com/FuelLabs/fuel-specs/issues/220
    pub da_height: DaBlockHeight,
    /// Generated application fields.
    pub generated: Generated,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(any(test, feature = "test-helpers"), derive(Default))]
/// Concrete generated application header fields.
/// These are generated once the full block has been run.
pub struct GeneratedApplicationFields {
    /// Number of transactions in this block.
    pub transactions_count: u64,
    /// Number of output messages in this block.
    pub output_messages_count: u64,
    /// Merkle root of transactions.
    pub transactions_root: Bytes32,
    /// Merkle root of messages in this block.
    pub output_messages_root: Bytes32,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// The fuel block consensus header.
/// This contains fields related to consensus plus
/// the hash of the [`FuelApplicationHeader`].
pub struct FuelConsensusHeader<Generated> {
    /// Merkle root of all previous block header hashes.
    pub prev_root: Bytes32,
    /// Fuel block height.
    pub height: BlockHeight,
    /// The block producer time.
    pub time: DateTime<Utc>,
    /// generated consensus fields.
    pub generated: Generated,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(any(test, feature = "test-helpers"), derive(Default))]
/// Concrete generated consensus header fields.
/// These are generated once the full block has been run.
pub struct GeneratedConsensusFields {
    /// Hash of the application header.
    pub application_hash: Bytes32,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Extra data that is not actually part of the header.
pub struct HeaderMetadata {
    /// Hash of the header.
    id: BlockId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsensusType {
    PoA,
}

// Accessors for the consensus header.
impl FuelBlockHeader {
    /// Merkle root of all previous block header hashes.
    pub fn prev_root(&self) -> &Bytes32 {
        &self.as_ref().prev_root
    }
    /// Fuel block height.
    pub fn height(&self) -> &BlockHeight {
        &self.as_ref().height
    }
    /// The block producer time.
    pub fn time(&self) -> &DateTime<Utc> {
        &self.as_ref().time
    }

    /// The hash of the application header.
    pub fn application_hash(&self) -> &Bytes32 {
        &self.as_ref().application_hash
    }

    /// The type of consensus this header is using.
    pub fn consensus_type(&self) -> ConsensusType {
        ConsensusType::PoA
    }
}

// Accessors for the consensus header.
impl PartialFuelBlockHeader {
    /// Merkle root of all previous block header hashes.
    pub fn prev_root(&self) -> &Bytes32 {
        &self.as_ref().prev_root
    }
    /// Fuel block height.
    pub fn height(&self) -> &BlockHeight {
        &self.as_ref().height
    }
    /// The block producer time.
    pub fn time(&self) -> &DateTime<Utc> {
        &self.as_ref().time
    }

    /// The type of consensus this header is using.
    pub fn consensus_type(&self) -> ConsensusType {
        ConsensusType::PoA
    }
}

impl FuelBlockHeader {
    /// Re-generate the header metadata.
    pub fn recalculate_metadata(&mut self) {
        self.metadata = Some(HeaderMetadata { id: self.hash() });
    }

    /// Get the hash of the fuel header.
    pub fn hash(&self) -> BlockId {
        // This internally hashes the hash of the application header.
        self.consensus.hash()
    }

    /// Get the cached fuel header hash.
    pub fn id(&self) -> BlockId {
        if let Some(ref metadata) = self.metadata {
            metadata.id
        } else {
            self.hash()
        }
    }
}

impl PartialFuelBlockHeader {
    /// Generate all fields to create a full [`FuelBlockHeader`]
    /// after running the transactions.
    ///
    /// The order of the transactions must be the same order they were
    /// executed in.
    /// The order of the messages must be the same as they were
    /// produced in.
    ///
    /// Message ids are produced by executed the transactions and collecting
    /// the ids from the receipts of messages outputs.
    ///
    /// The transactions are the bytes of the executed [`Transaction`]s.
    pub fn generate(
        self,
        transactions: &[Vec<u8>],
        message_ids: &[MessageId],
    ) -> FuelBlockHeader {
        // Generate the transaction merkle root.
        let mut transaction_tree = fuel_merkle::binary::in_memory::MerkleTree::new();
        for id in transactions {
            transaction_tree.push(id.as_ref());
        }
        let transactions_root = transaction_tree.root().into();

        // Generate the message merkle root.
        let mut message_tree = fuel_merkle::binary::in_memory::MerkleTree::new();
        for id in message_ids {
            message_tree.push(id.as_ref());
        }
        let output_messages_root = message_tree.root().into();

        let application = FuelApplicationHeader {
            da_height: self.application.da_height,
            generated: GeneratedApplicationFields {
                transactions_count: transactions.len() as u64,
                output_messages_count: message_ids.len() as u64,
                transactions_root,
                output_messages_root,
            },
        };

        // Generate the hash of the complete application header.
        let application_hash = application.hash();
        let mut header = FuelBlockHeader {
            application,
            consensus: FuelConsensusHeader {
                prev_root: self.consensus.prev_root,
                height: self.consensus.height,
                time: self.consensus.time,
                generated: GeneratedConsensusFields { application_hash },
            },
            metadata: None,
        };

        // cache the hash.
        header.recalculate_metadata();
        header
    }

    /// Creates a [`FuelBlockHeader`] that has the
    /// generated fields set to meaningless values.
    fn without_generated(self) -> FuelBlockHeader {
        FuelBlockHeader {
            application: FuelApplicationHeader {
                da_height: self.application.da_height,
                generated: GeneratedApplicationFields {
                    transactions_count: Default::default(),
                    output_messages_count: Default::default(),
                    transactions_root: Default::default(),
                    output_messages_root: Default::default(),
                },
            },
            consensus: FuelConsensusHeader {
                prev_root: self.consensus.prev_root,
                height: self.consensus.height,
                time: self.consensus.time,
                generated: GeneratedConsensusFields {
                    application_hash: Default::default(),
                },
            },
            metadata: None,
        }
    }
}

impl FuelApplicationHeader<GeneratedApplicationFields> {
    /// Hash the application header.
    fn hash(&self) -> Bytes32 {
        // Order matters and is the same as the spec.
        let mut hasher = Hasher::default();
        hasher.input(&self.da_height.to_bytes()[..]);
        hasher.input(self.transactions_count.to_be_bytes());
        hasher.input(self.output_messages_count.to_be_bytes());
        hasher.input(self.transactions_root.as_ref());
        hasher.input(self.output_messages_root.as_ref());
        hasher.digest()
    }
}

impl FuelConsensusHeader<GeneratedConsensusFields> {
    /// Hash the consensus header.
    fn hash(&self) -> BlockId {
        // Order matters and is the same as the spec.
        let mut hasher = Hasher::default();
        hasher.input(self.prev_root.as_ref());
        hasher.input(&self.height.to_bytes()[..]);
        hasher.input(self.time.timestamp_millis().to_be_bytes());
        hasher.input(self.application_hash.as_ref());
        BlockId(hasher.digest())
    }
}

impl core::ops::Deref for FuelBlockHeader {
    type Target = FuelApplicationHeader<GeneratedApplicationFields>;

    fn deref(&self) -> &Self::Target {
        &self.application
    }
}

impl core::ops::Deref for PartialFuelBlockHeader {
    type Target = FuelApplicationHeader<Empty>;

    fn deref(&self) -> &Self::Target {
        &self.application
    }
}

impl core::ops::Deref for FuelApplicationHeader<GeneratedApplicationFields> {
    type Target = GeneratedApplicationFields;

    fn deref(&self) -> &Self::Target {
        &self.generated
    }
}

impl core::ops::Deref for FuelConsensusHeader<GeneratedConsensusFields> {
    type Target = GeneratedConsensusFields;

    fn deref(&self) -> &Self::Target {
        &self.generated
    }
}

impl core::convert::AsRef<FuelConsensusHeader<GeneratedConsensusFields>>
    for FuelBlockHeader
{
    fn as_ref(&self) -> &FuelConsensusHeader<GeneratedConsensusFields> {
        &self.consensus
    }
}

impl core::convert::AsRef<FuelConsensusHeader<Empty>> for PartialFuelBlockHeader {
    fn as_ref(&self) -> &FuelConsensusHeader<Empty> {
        &self.consensus
    }
}

/// The compact representation of a block used in the database
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(any(test, feature = "test-helpers"), derive(Default))]
pub struct FuelBlockDb {
    pub header: FuelBlockHeader,
    pub transactions: Vec<Bytes32>,
}

impl FuelBlockDb {
    /// Hash of the header.
    pub fn id(&self) -> BlockId {
        self.header.id()
    }

    /// The type of consensus this header is using.
    pub fn consensus_type(&self) -> ConsensusType {
        self.header.consensus_type()
    }

    /// Hack until we completely remove the need for meaningless
    /// default headers.
    pub fn fix_me_default_block() -> Self {
        Self {
            header: FuelBlockHeader {
                application: FuelApplicationHeader {
                    da_height: Default::default(),
                    generated: GeneratedApplicationFields {
                        transactions_count: Default::default(),
                        output_messages_count: Default::default(),
                        transactions_root: Default::default(),
                        output_messages_root: Default::default(),
                    },
                },
                consensus: FuelConsensusHeader {
                    prev_root: Default::default(),
                    height: Default::default(),
                    time: Default::default(),
                    generated: GeneratedConsensusFields {
                        application_hash: Default::default(),
                    },
                },
                metadata: None,
            },
            transactions: Default::default(),
        }
    }
}

/// Fuel block with all transaction data included
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(any(test, feature = "test-helpers"), derive(Default))]
pub struct FuelBlock {
    /// Generated complete header.
    header: FuelBlockHeader,
    /// Executed transactions.
    transactions: Vec<Transaction>,
}

/// Fuel block with all transaction data included
/// but without any data generated.
/// This type can be created with unexecuted
/// transactions to produce a [`FuelBlock`] or
/// it can be created with pre-executed transactions in
/// order to validate they were constructed correctly.
#[derive(Clone, Debug)]
pub struct PartialFuelBlock {
    /// The partial header.
    pub header: PartialFuelBlockHeader,
    /// Transactions that can either be pre-executed
    /// or not.
    pub transactions: Vec<Transaction>,
}

impl FuelBlock {
    /// Create a new full fuel block from a [`PartialFuelBlockHeader`],
    /// executed transactions and the [`MessageId`]s.
    ///
    /// The order of the transactions must be the same order they were
    /// executed in.
    /// The order of the messages must be the same as they were
    /// produced in.
    ///
    /// Message ids are produced by executed the transactions and collecting
    /// the ids from the receipts of messages outputs.
    pub fn new(
        header: PartialFuelBlockHeader,
        mut transactions: Vec<Transaction>,
        message_ids: &[MessageId],
    ) -> Self {
        // I think this is safe as it doesn't appear that any of the reads actually mutate the data.
        // Alternatively we can clone to be safe.
        let transaction_ids: Vec<_> =
            transactions.iter_mut().map(|tx| tx.to_bytes()).collect();
        Self {
            header: header.generate(&transaction_ids[..], message_ids),
            transactions,
        }
    }

    /// Get the hash of the header.
    pub fn id(&self) -> BlockId {
        self.header.id()
    }

    /// Create a database friendly fuel block.
    pub fn to_db_block(&self) -> FuelBlockDb {
        FuelBlockDb {
            header: self.header.clone(),
            transactions: self.transactions.iter().map(|tx| tx.id()).collect(),
        }
    }

    /// Convert from a previously stored block back to a full block
    pub fn from_db_block(db_block: FuelBlockDb, transactions: Vec<Transaction>) -> Self {
        // TODO: should we perform an extra validation step to ensure the provided
        //  txs match the expected ones in the block?
        Self {
            header: db_block.header,
            transactions,
        }
    }

    /// Get the executed transactions.
    pub fn transactions(&self) -> &[Transaction] {
        &self.transactions[..]
    }

    /// Get the complete header.
    pub fn header(&self) -> &FuelBlockHeader {
        &self.header
    }

    #[cfg(any(test, feature = "test-helpers"))]
    pub fn transactions_mut(&mut self) -> &mut Vec<Transaction> {
        &mut self.transactions
    }

    #[cfg(any(test, feature = "test-helpers"))]
    pub fn header_mut(&mut self) -> &mut FuelBlockHeader {
        &mut self.header
    }
}

impl PartialFuelBlock {
    pub fn new(header: PartialFuelBlockHeader, transactions: Vec<Transaction>) -> Self {
        Self {
            header,
            transactions,
        }
    }

    /// Creates a [`FuelBlockDb`] that has the
    /// generated fields set to meaningless values.
    ///
    /// Hack until we figure out a better way to represent
    /// a block in the database that hasn't been run.
    pub fn to_partial_db_block(&self) -> FuelBlockDb {
        FuelBlockDb {
            header: self.header.clone().without_generated(),
            transactions: self.transactions.iter().map(|tx| tx.id()).collect(),
        }
    }

    /// Generate a [`FuelBlock`] after running this partial block.
    ///
    /// The order of the messages must be the same as they were
    /// produced in.
    ///
    /// Message ids are produced by executed the transactions and collecting
    /// the ids from the receipts of messages outputs.
    pub fn generate(self, message_ids: &[MessageId]) -> FuelBlock {
        FuelBlock::new(self.header, self.transactions, message_ids)
    }
}

impl From<FuelBlock> for PartialFuelBlock {
    fn from(block: FuelBlock) -> Self {
        let FuelBlock {
            header:
                FuelBlockHeader {
                    application: FuelApplicationHeader { da_height, .. },
                    consensus:
                        FuelConsensusHeader {
                            prev_root,
                            height,
                            time,
                            ..
                        },
                    ..
                },
            transactions,
        } = block;
        Self {
            header: PartialFuelBlockHeader {
                application: FuelApplicationHeader {
                    da_height,
                    generated: Empty {},
                },
                consensus: FuelConsensusHeader {
                    prev_root,
                    height,
                    time,
                    generated: Empty {},
                },
                metadata: None,
            },
            transactions,
        }
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// The consensus related data that doesn't live on the
/// header.
pub enum FuelBlockConsensus {
    PoA(FuelBlockPoAConsensus),
}

impl FuelBlockConsensus {
    /// Retrieve the block producer address from the consensus data
    pub fn block_producer(&self, block_id: &BlockId) -> anyhow::Result<Address> {
        match &self {
            FuelBlockConsensus::PoA(poa_data) => {
                let public_key = poa_data.signature.recover(block_id.as_message())?;
                let address = Input::owner(&public_key);
                Ok(address)
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// The consensus related data that doesn't live on the
/// header.
pub struct FuelBlockPoAConsensus {
    /// The signature of the [`FuelBlockHeader`].
    pub signature: Signature,
}

#[cfg(any(test, feature = "test-helpers"))]
impl<T> Default for FuelConsensusHeader<T>
where
    T: Default,
{
    fn default() -> Self {
        Self {
            time: Utc.timestamp(0, 0),
            height: BlockHeight::default(),
            prev_root: Bytes32::default(),
            generated: Default::default(),
        }
    }
}

#[cfg(any(test, feature = "test-helpers"))]
impl Default for FuelBlockConsensus {
    fn default() -> Self {
        FuelBlockConsensus::PoA(Default::default())
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(any(test, feature = "test-helpers"), derive(Default))]
/// A fuel block with the related consensus data.
pub struct SealedFuelBlock {
    pub block: FuelBlock,
    pub consensus: FuelBlockConsensus,
}

impl FuelBlockPoAConsensus {
    /// Create a new block consensus.
    pub fn new(signature: Signature) -> Self {
        Self { signature }
    }
}

impl SealedFuelBlock {
    //// Hack until we can completely remove meaningless
    //// default headers.
    pub fn fix_me_default_block() -> Self {
        Self {
            block: FuelBlock {
                header: FuelBlockHeader {
                    application: FuelApplicationHeader {
                        da_height: Default::default(),
                        generated: GeneratedApplicationFields {
                            transactions_count: Default::default(),
                            output_messages_count: Default::default(),
                            transactions_root: Default::default(),
                            output_messages_root: Default::default(),
                        },
                    },
                    consensus: FuelConsensusHeader {
                        prev_root: Default::default(),
                        height: Default::default(),
                        time: Default::default(),
                        generated: GeneratedConsensusFields {
                            application_hash: Default::default(),
                        },
                    },
                    metadata: None,
                },
                transactions: Default::default(),
            },
            consensus: FuelBlockConsensus::PoA(FuelBlockPoAConsensus {
                signature: Default::default(),
            }),
        }
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(any(test, feature = "test-helpers"), derive(Default))]
/// A fuel block with the related consensus data.
pub struct SealedFuelBlockHeader {
    pub header: FuelBlockHeader,
    pub consensus: FuelBlockConsensus,
}
