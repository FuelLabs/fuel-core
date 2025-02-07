//! Block header types

mod v1;

#[cfg(feature = "fault-proving")]
mod v2;

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
use enum_dispatch::enum_dispatch;
use tai64::Tai64;
pub use v1::BlockHeaderV1;
#[cfg(feature = "fault-proving")]
pub use v2::{
    BlockHeaderV2,
    FaultProvingHeader,
};

/// Version-able block header type
#[derive(Clone, Debug, derivative::Derivative)]
#[derivative(PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[enum_dispatch(GetBlockHeaderFields)]
#[cfg_attr(
    any(test, feature = "test-helpers"),
    enum_dispatch(BlockHeaderDataTestHelpers)
)]
pub enum BlockHeader {
    /// V1 BlockHeader
    V1(BlockHeaderV1),
    /// V2 BlockHeader
    #[cfg(feature = "fault-proving")]
    V2(BlockHeaderV2),
}

/// Helpful methods for all variants of the block header.
#[enum_dispatch]
pub(crate) trait GetBlockHeaderFields {
    /// Get the consensus portion of the header.
    fn consensus(&self) -> &ConsensusHeader<GeneratedConsensusFields>;
    /// Get the application portion of the header.
    fn application(&self) -> &ApplicationHeader<GeneratedApplicationFields>;
    /// Get the metadata of the header.
    fn metadata(&self) -> &Option<BlockHeaderMetadata>;
    /// Re-generate the header metadata.
    fn recalculate_metadata(&mut self);
    /// Get the hash of the fuel header.
    fn hash(&self) -> BlockId;
    /// Get the transaction ID Commitment
    fn tx_id_commitment(&self) -> Option<Bytes32>;
}

// reimplement GetBlockHeaderFields but with plain impl
// since both v1 and v2 support the fields, each function can just do
// GetBlockHeaderFields::fn(&self)
impl BlockHeader {
    /// Get the consensus portion of the header.
    pub fn consensus(&self) -> &ConsensusHeader<GeneratedConsensusFields> {
        GetBlockHeaderFields::consensus(self)
    }

    /// Get the application portion of the header.
    pub fn application(&self) -> &ApplicationHeader<GeneratedApplicationFields> {
        GetBlockHeaderFields::application(self)
    }

    /// Get the metadata of the header.
    pub fn metadata(&self) -> &Option<BlockHeaderMetadata> {
        GetBlockHeaderFields::metadata(self)
    }

    /// Re-generate the header metadata.
    pub fn recalculate_metadata(&mut self) {
        GetBlockHeaderFields::recalculate_metadata(self)
    }

    /// Get the hash of the fuel header.
    pub fn hash(&self) -> BlockId {
        GetBlockHeaderFields::hash(self)
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
        let transactions_root = generate_txns_root(transactions);

        transactions_root == self.application().transactions_root
            && transactions.len() == self.application().transactions_count as usize
    }

    /// Getter for the tx id commitment
    pub fn tx_id_commitment(&self) -> Option<Bytes32> {
        GetBlockHeaderFields::tx_id_commitment(self)
    }
}

/// Helpful test methods for all variants of the block header.
#[cfg(any(test, feature = "test-helpers"))]
#[enum_dispatch]
pub(crate) trait BlockHeaderDataTestHelpers {
    /// Mutable getter for consensus portion of header
    fn consensus_mut(&mut self) -> &mut ConsensusHeader<GeneratedConsensusFields>;
    /// Set the entire consensus header
    fn set_consensus_header(
        &mut self,
        consensus: ConsensusHeader<GeneratedConsensusFields>,
    );
    /// Mutable getter for application portion of header
    fn application_mut(&mut self) -> &mut ApplicationHeader<GeneratedApplicationFields>;
    /// Set the entire application header
    fn set_application_header(
        &mut self,
        application: ApplicationHeader<GeneratedApplicationFields>,
    );
    /// Set the block height for the header
    fn set_block_height(&mut self, height: BlockHeight);
    /// Set the previous root for the header
    fn set_previous_root(&mut self, root: Bytes32);
    /// Set the time for the header
    fn set_time(&mut self, time: Tai64);
    /// Set the transaction root for the header
    fn set_transaction_root(&mut self, root: Bytes32);
    /// Set the DA height for the header
    fn set_da_height(&mut self, da_height: DaBlockHeight);
}

/// Reimplement helpers so that its easy to import
#[cfg(any(test, feature = "test-helpers"))]
impl BlockHeader {
    /// Mutable getter for consensus portion of header
    pub fn consensus_mut(&mut self) -> &mut ConsensusHeader<GeneratedConsensusFields> {
        BlockHeaderDataTestHelpers::consensus_mut(self)
    }

    /// Set the entire consensus header
    pub fn set_consensus_header(
        &mut self,
        consensus: ConsensusHeader<GeneratedConsensusFields>,
    ) {
        BlockHeaderDataTestHelpers::set_consensus_header(self, consensus)
    }

    /// Mutable getter for application portion of header
    pub fn application_mut(
        &mut self,
    ) -> &mut ApplicationHeader<GeneratedApplicationFields> {
        BlockHeaderDataTestHelpers::application_mut(self)
    }

    /// Set the entire application header
    pub fn set_application_header(
        &mut self,
        application: ApplicationHeader<GeneratedApplicationFields>,
    ) {
        BlockHeaderDataTestHelpers::set_application_header(self, application)
    }

    /// Set the block height for the header
    pub fn set_block_height(&mut self, height: BlockHeight) {
        BlockHeaderDataTestHelpers::set_block_height(self, height)
    }

    /// Set the previous root for the header
    pub fn set_previous_root(&mut self, root: Bytes32) {
        BlockHeaderDataTestHelpers::set_previous_root(self, root)
    }

    /// Set the time for the header
    pub fn set_time(&mut self, time: Tai64) {
        BlockHeaderDataTestHelpers::set_time(self, time)
    }

    /// Set the transaction root for the header
    pub fn set_transaction_root(&mut self, root: Bytes32) {
        BlockHeaderDataTestHelpers::set_transaction_root(self, root)
    }

    /// Set the DA height for the header
    pub fn set_da_height(&mut self, da_height: DaBlockHeight) {
        BlockHeaderDataTestHelpers::set_da_height(self, da_height)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Default)]
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
pub const LATEST_STATE_TRANSITION_VERSION: StateTransitionBytecodeVersion = 21;

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
        let mut default: BlockHeader = BlockHeaderV1::default().into();
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
        #[cfg(feature = "fault-proving")] chain_id: &crate::fuel_types::ChainId,
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

        #[cfg(not(feature = "fault-proving"))]
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

        #[cfg(feature = "fault-proving")]
        let mut header: BlockHeader = BlockHeaderV2 {
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
            fault_proving: FaultProvingHeader {
                tx_id_commitment: v2::generate_tx_id_commitment(transactions, chain_id),
            },
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
