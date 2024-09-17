//! Block header types

use super::{
    consensus::ConsensusType,
    primitives::{
        BlockId,
        DaBlockHeight,
        Empty,
    },
};
use crate::{
    fuel_merkle,
    fuel_merkle::binary::root_calculator::MerkleRootCalculator,
    fuel_tx::Transaction,
    fuel_types::{
        canonical::Serialize,
        BlockHeight,
        Bytes32,
        MessageId,
    },
};
use tai64::Tai64;

/// Version-able block header type
#[derive(Clone, Debug, derivative::Derivative)]
#[derivative(PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum BlockHeader {
    /// V1 BlockHeader
    V1(BlockHeaderV1),
}

/// A fuel block header that has all the fields generated because it
/// has been executed.
#[derive(Clone, Debug, derivative::Derivative)]
#[derivative(PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BlockHeaderV1 {
    /// The application header.
    pub application: ApplicationHeader<GeneratedApplicationFields>,
    /// The consensus header.
    pub consensus: ConsensusHeader<GeneratedConsensusFields>,
    /// The header metadata calculated during creation.
    /// The field is private to enforce the use of the [`PartialBlockHeader::generate`] method.
    #[cfg_attr(feature = "serde", serde(skip))]
    #[derivative(PartialEq = "ignore")]
    metadata: Option<BlockHeaderMetadata>,
}

impl From<BlockHeaderV1> for BlockHeader {
    fn from(v1: BlockHeaderV1) -> Self {
        BlockHeader::V1(v1)
    }
}

impl BlockHeader {
    /// Getter for consensus portion of header
    pub fn consensus(&self) -> &ConsensusHeader<GeneratedConsensusFields> {
        match self {
            BlockHeader::V1(v1) => &v1.consensus,
        }
    }

    /// Getter for application portion of header
    pub fn application(&self) -> &ApplicationHeader<GeneratedApplicationFields> {
        match self {
            BlockHeader::V1(v1) => &v1.application,
        }
    }

    /// Getter for metadata portion of header
    fn metadata(&self) -> &Option<BlockHeaderMetadata> {
        match self {
            BlockHeader::V1(v1) => &v1.metadata,
        }
    }

    fn _consensus_mut(&mut self) -> &mut ConsensusHeader<GeneratedConsensusFields> {
        match self {
            BlockHeader::V1(v1) => &mut v1.consensus,
        }
    }
}

#[cfg(feature = "test-helpers")]
impl BlockHeader {
    /// Mutable getter for consensus portion of header
    pub fn consensus_mut(&mut self) -> &mut ConsensusHeader<GeneratedConsensusFields> {
        self._consensus_mut()
    }
    /// Set the entire consensus header
    pub fn set_consensus_header(
        &mut self,
        consensus: ConsensusHeader<GeneratedConsensusFields>,
    ) {
        match self {
            BlockHeader::V1(v1) => v1.consensus = consensus,
        }
    }

    /// Mutable getter for application portion of header
    pub fn application_mut(
        &mut self,
    ) -> &mut ApplicationHeader<GeneratedApplicationFields> {
        match self {
            BlockHeader::V1(v1) => &mut v1.application,
        }
    }

    /// Set the entire application header
    pub fn set_application_header(
        &mut self,
        application: ApplicationHeader<GeneratedApplicationFields>,
    ) {
        match self {
            BlockHeader::V1(v1) => v1.application = application,
        }
    }

    /// Set the block height for the header
    pub fn set_block_height(&mut self, height: BlockHeight) {
        self.consensus_mut().height = height;
        self.recalculate_metadata();
    }

    /// Set the previous root for the header
    pub fn set_previous_root(&mut self, root: Bytes32) {
        self.consensus_mut().prev_root = root;
        self.recalculate_metadata();
    }

    /// Set the time for the header
    pub fn set_time(&mut self, time: Tai64) {
        self.consensus_mut().time = time;
        self.recalculate_metadata();
    }

    /// Set the transaction root for the header
    pub fn set_transaction_root(&mut self, root: Bytes32) {
        self.application_mut().generated.transactions_root = root;
        self.recalculate_metadata();
    }

    /// Set the DA height for the header
    pub fn set_da_height(&mut self, da_height: DaBlockHeight) {
        self.application_mut().da_height = da_height;
        self.recalculate_metadata();
    }
}

#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(any(test, feature = "test-helpers"), derive(Default))]
/// A partially complete fuel block header that does not
/// have any generated fields because it has not been executed yet.
pub struct PartialBlockHeader {
    /// The application header.
    pub application: ApplicationHeader<Empty>,
    /// The consensus header.
    pub consensus: ConsensusHeader<Empty>,
}

/// The type representing the version of the consensus parameters.
pub type ConsensusParametersVersion = u32;
/// The type representing the version of the state transition bytecode.
pub type StateTransitionBytecodeVersion = u32;

/// The latest version of the state transition bytecode.
pub const LATEST_STATE_TRANSITION_VERSION: StateTransitionBytecodeVersion = 10;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// The fuel block application header.
/// Contains everything except consensus related data.
pub struct ApplicationHeader<Generated> {
    /// The layer 1 height of messages and events to include since the last layer 1 block number.
    /// This is not meant to represent the layer 1 block this was committed to. Validators will need
    /// to have some rules in place to ensure the block number was chosen in a reasonable way. For
    /// example, they should verify that the block number satisfies the finality requirements of the
    /// layer 1 chain. They should also verify that the block number isn't too stale and is increasing.
    /// Some similar concerns are noted in this issue: <https://github.com/FuelLabs/fuel-specs/issues/220>
    pub da_height: DaBlockHeight,
    /// The version of the consensus parameters used to execute this block.
    pub consensus_parameters_version: ConsensusParametersVersion,
    /// The version of the state transition bytecode used to execute this block.
    pub state_transition_bytecode_version: StateTransitionBytecodeVersion,
    /// Generated application fields.
    pub generated: Generated,
}

#[cfg(any(test, feature = "test-helpers"))]
impl<Generated> Default for ApplicationHeader<Generated>
where
    Generated: Default,
{
    fn default() -> Self {
        Self {
            da_height: Default::default(),
            consensus_parameters_version: Default::default(),
            state_transition_bytecode_version: LATEST_STATE_TRANSITION_VERSION,
            generated: Default::default(),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(any(test, feature = "test-helpers"), derive(Default))]
/// Concrete generated application header fields.
/// These are generated once the full block has been run.
pub struct GeneratedApplicationFields {
    /// Number of transactions in this block.
    pub transactions_count: u16,
    /// Number of message receipts in this block.
    pub message_receipt_count: u32,
    /// Merkle root of transactions.
    pub transactions_root: Bytes32,
    /// Merkle root of message receipts in this block.
    pub message_outbox_root: Bytes32,
    /// Root hash of all imported events from L1
    pub event_inbox_root: Bytes32,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// The fuel block consensus header.
/// This contains fields related to consensus plus
/// the hash of the [`ApplicationHeader`].
pub struct ConsensusHeader<Generated> {
    /// Merkle root of all previous block header hashes.
    pub prev_root: Bytes32,
    /// Fuel block height.
    pub height: BlockHeight,
    /// The block producer time.
    pub time: Tai64,
    /// generated consensus fields.
    pub generated: Generated,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(any(test, feature = "test-helpers"), derive(Default))]
/// Concrete generated consensus header fields.
/// These are generated once the full block has been run.
pub struct GeneratedConsensusFields {
    /// Hash of the application header.
    pub application_hash: Bytes32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Extra data that is not actually part of the header.
pub struct BlockHeaderMetadata {
    /// Hash of the header.
    id: BlockId,
}

#[cfg(any(test, feature = "test-helpers"))]
impl Default for BlockHeader {
    fn default() -> Self {
        let mut default: BlockHeader = BlockHeaderV1 {
            application: Default::default(),
            consensus: Default::default(),
            metadata: None,
        }
        .into();
        default.recalculate_metadata();
        default
    }
}

#[cfg(any(test, feature = "test-helpers"))]
impl BlockHeader {
    /// Creates the block header around the `height` and `time`.
    /// The method should be used only for tests.
    pub fn new_block(height: BlockHeight, time: Tai64) -> Self {
        let mut default = Self::default();
        default.consensus_mut().height = height;
        default.consensus_mut().time = time;
        default.recalculate_metadata();
        default
    }
}

// Accessors for the consensus header.
impl BlockHeader {
    /// Merkle root of all previous block header hashes.
    pub fn prev_root(&self) -> &Bytes32 {
        &self.as_ref().prev_root
    }
    /// Fuel block height.
    pub fn height(&self) -> &BlockHeight {
        &self.as_ref().height
    }
    /// The block producer time.
    pub fn time(&self) -> Tai64 {
        self.as_ref().time
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

/// Accessors for the consensus header.
impl PartialBlockHeader {
    /// Merkle root of all previous block header hashes.
    pub fn prev_root(&self) -> &Bytes32 {
        &self.as_ref().prev_root
    }
    /// Fuel block height.
    pub fn height(&self) -> &BlockHeight {
        &self.as_ref().height
    }
    /// The block producer time.
    pub fn time(&self) -> &Tai64 {
        &self.as_ref().time
    }
    /// The type of consensus this header is using.
    pub fn consensus_type(&self) -> ConsensusType {
        ConsensusType::PoA
    }
}
impl From<&BlockHeader> for PartialBlockHeader {
    fn from(header: &BlockHeader) -> Self {
        let ConsensusHeader {
            prev_root,
            height,
            time,
            ..
        } = *header.consensus();
        PartialBlockHeader {
            application: ApplicationHeader {
                da_height: header.da_height,
                consensus_parameters_version: header.consensus_parameters_version,
                state_transition_bytecode_version: header
                    .state_transition_bytecode_version,
                generated: Empty {},
            },
            consensus: ConsensusHeader {
                prev_root,
                height,
                time,
                generated: Empty {},
            },
        }
    }
}

impl BlockHeader {
    /// Re-generate the header metadata.
    pub fn recalculate_metadata(&mut self) {
        let application_hash = self.application().hash();
        self._consensus_mut().generated.application_hash = application_hash;
        let id = self.hash();
        match self {
            BlockHeader::V1(v1) => {
                v1.metadata = Some(BlockHeaderMetadata { id });
            }
        }
    }

    /// Get the hash of the fuel header.
    pub fn hash(&self) -> BlockId {
        // The `BlockHeader` can be created only via the [`PartialBlockHeader::generate`] method,
        // which calculates the hash of the `ApplicationHeader`. So the block header is immutable
        // and can't change its final hash on the fly.
        //
        // This assertion is a double-checks that this behavior is not changed.
        debug_assert_eq!(self.application_hash(), &self.application().hash());
        // This internally hashes the hash of the application header.
        self.consensus().hash()
    }

    /// Get the cached fuel header hash.
    pub fn id(&self) -> BlockId {
        if let Some(ref metadata) = self.metadata() {
            metadata.id
        } else {
            self.hash()
        }
    }

    /// Validate the transactions match the header.
    pub fn validate_transactions(&self, transactions: &[Transaction]) -> bool {
        // Generate the transaction merkle root.
        let transactions_root = generate_txns_root(transactions);

        transactions_root == self.application().transactions_root
            && transactions.len() == self.application().transactions_count as usize
    }
}

/// Error type for generating a full [`BlockHeader`] from a [`PartialBlockHeader`].
#[derive(derive_more::Display, Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[allow(missing_docs)]
pub enum BlockHeaderError {
    #[display(fmt = "The block header has too many transactions to fit into the `u16`")]
    TooManyTransactions,
    #[display(fmt = "The block header has too many messages to fit into the `u32`")]
    TooManyMessages,
}

impl PartialBlockHeader {
    /// Generate all fields to create a full [`BlockHeader`]
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
        transactions: &[Transaction],
        outbox_message_ids: &[MessageId],
        event_inbox_root: Bytes32,
    ) -> Result<BlockHeader, BlockHeaderError> {
        // Generate the transaction merkle root.
        let transactions_root = generate_txns_root(transactions);

        // Generate the message merkle root.
        let message_outbox_root = outbox_message_ids
            .iter()
            .fold(MerkleRootCalculator::new(), |mut tree, id| {
                tree.push(id.as_ref());
                tree
            })
            .root()
            .into();

        let application = ApplicationHeader {
            da_height: self.application.da_height,
            consensus_parameters_version: self.application.consensus_parameters_version,
            state_transition_bytecode_version: self
                .application
                .state_transition_bytecode_version,
            generated: GeneratedApplicationFields {
                transactions_count: u16::try_from(transactions.len())
                    .map_err(|_| BlockHeaderError::TooManyTransactions)?,
                message_receipt_count: u32::try_from(outbox_message_ids.len())
                    .map_err(|_| BlockHeaderError::TooManyMessages)?,
                transactions_root,
                message_outbox_root,
                event_inbox_root,
            },
        };

        let mut header: BlockHeader = BlockHeaderV1 {
            application,
            consensus: ConsensusHeader {
                prev_root: self.consensus.prev_root,
                height: self.consensus.height,
                time: self.consensus.time,
                generated: GeneratedConsensusFields {
                    // Calculates it inside of `BlockHeader::recalculate_metadata`.
                    application_hash: Default::default(),
                },
            },
            metadata: None,
        }
        .into();

        // Cache the hash.
        header.recalculate_metadata();
        Ok(header)
    }
}

fn generate_txns_root(transactions: &[Transaction]) -> Bytes32 {
    let transaction_ids = transactions.iter().map(|tx| tx.to_bytes());
    // Generate the transaction merkle root.
    let mut transaction_tree =
        fuel_merkle::binary::root_calculator::MerkleRootCalculator::new();
    for id in transaction_ids {
        transaction_tree.push(id.as_ref());
    }
    transaction_tree.root().into()
}

impl ApplicationHeader<GeneratedApplicationFields> {
    /// Hash the application header.
    pub fn hash(&self) -> Bytes32 {
        // Order matters and is the same as the spec.
        let mut hasher = crate::fuel_crypto::Hasher::default();
        let Self {
            da_height,
            consensus_parameters_version,
            state_transition_bytecode_version,
            generated:
                GeneratedApplicationFields {
                    transactions_count,
                    message_receipt_count,
                    transactions_root,
                    message_outbox_root,
                    event_inbox_root,
                },
        } = self;

        hasher.input(da_height.to_be_bytes());
        hasher.input(consensus_parameters_version.to_be_bytes());
        hasher.input(state_transition_bytecode_version.to_be_bytes());

        hasher.input(transactions_count.to_be_bytes());
        hasher.input(message_receipt_count.to_be_bytes());
        hasher.input(transactions_root.as_ref());
        hasher.input(message_outbox_root.as_ref());
        hasher.input(event_inbox_root.as_ref());

        hasher.digest()
    }
}

impl ConsensusHeader<GeneratedConsensusFields> {
    /// Hash the consensus header.
    pub fn hash(&self) -> BlockId {
        // Order matters and is the same as the spec.
        let mut hasher = crate::fuel_crypto::Hasher::default();
        let Self {
            prev_root,
            height,
            time,
            generated: GeneratedConsensusFields { application_hash },
        } = self;

        hasher.input(prev_root.as_ref());
        hasher.input(height.to_be_bytes());
        hasher.input(time.0.to_be_bytes());

        hasher.input(application_hash.as_ref());

        BlockId::from(hasher.digest())
    }
}

#[cfg(any(test, feature = "test-helpers"))]
impl<T> Default for ConsensusHeader<T>
where
    T: Default,
{
    fn default() -> Self {
        Self {
            time: Tai64::UNIX_EPOCH,
            height: BlockHeight::default(),
            prev_root: Bytes32::default(),
            generated: Default::default(),
        }
    }
}

impl core::ops::Deref for BlockHeader {
    type Target = ApplicationHeader<GeneratedApplicationFields>;

    fn deref(&self) -> &Self::Target {
        self.application()
    }
}

impl core::ops::Deref for PartialBlockHeader {
    type Target = ApplicationHeader<Empty>;

    fn deref(&self) -> &Self::Target {
        &self.application
    }
}

impl core::ops::Deref for ApplicationHeader<GeneratedApplicationFields> {
    type Target = GeneratedApplicationFields;

    fn deref(&self) -> &Self::Target {
        &self.generated
    }
}

impl core::ops::Deref for ConsensusHeader<GeneratedConsensusFields> {
    type Target = GeneratedConsensusFields;

    fn deref(&self) -> &Self::Target {
        &self.generated
    }
}

impl core::convert::AsRef<ConsensusHeader<GeneratedConsensusFields>> for BlockHeader {
    fn as_ref(&self) -> &ConsensusHeader<GeneratedConsensusFields> {
        self.consensus()
    }
}

impl core::convert::AsRef<ConsensusHeader<Empty>> for PartialBlockHeader {
    fn as_ref(&self) -> &ConsensusHeader<Empty> {
        &self.consensus
    }
}
