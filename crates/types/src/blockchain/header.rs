//! Block header types

/// The V1 version of the header.
pub mod v1;

/// The V2 version of the header.
#[cfg(feature = "fault-proving")]
pub mod v2;

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
        BlockHeight,
        Bytes32,
        MessageId,
        canonical::Serialize,
    },
};
use educe::Educe;
use tai64::Tai64;
pub use v1::BlockHeaderV1;
use v1::GeneratedApplicationFieldsV1;
#[cfg(feature = "fault-proving")]
pub use v2::BlockHeaderV2;
#[cfg(feature = "fault-proving")]
use v2::GeneratedApplicationFieldsV2;

/// Version-able block header type
#[derive(Clone, Debug, Educe)]
#[educe(PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum BlockHeader {
    /// V1 BlockHeader
    V1(BlockHeaderV1),
    /// V2 BlockHeader
    #[cfg(feature = "fault-proving")]
    V2(BlockHeaderV2),
}

impl BlockHeader {
    /// Get the da height
    pub fn da_height(&self) -> DaBlockHeight {
        match self {
            BlockHeader::V1(header) => header.application().da_height,
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => header.application().da_height,
        }
    }

    /// Get the consensus parameters version
    pub fn consensus_parameters_version(&self) -> ConsensusParametersVersion {
        match self {
            BlockHeader::V1(header) => header.application().consensus_parameters_version,
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => header.application().consensus_parameters_version,
        }
    }

    /// Get the state transition bytecode version
    pub fn state_transition_bytecode_version(&self) -> StateTransitionBytecodeVersion {
        match self {
            BlockHeader::V1(header) => {
                header.application().state_transition_bytecode_version
            }
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => {
                header.application().state_transition_bytecode_version
            }
        }
    }

    /// Get the consensus portion of the header.
    pub fn consensus(&self) -> &ConsensusHeader<GeneratedConsensusFields> {
        match self {
            BlockHeader::V1(header) => header.consensus(),
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => header.consensus(),
        }
    }

    /// Get the Block ID
    pub fn id(&self) -> BlockId {
        match self {
            BlockHeader::V1(header) => header.id(),
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => header.id(),
        }
    }

    /// Validate the transactions
    pub fn validate_transactions(&self, transactions: &[Transaction]) -> bool {
        match self {
            BlockHeader::V1(header) => header.validate_transactions(transactions),
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => header.validate_transactions(transactions),
        }
    }

    /// Recalculate the metadata
    pub fn recalculate_metadata(&mut self) {
        match self {
            BlockHeader::V1(header) => header.recalculate_metadata(),
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => header.recalculate_metadata(),
        }
    }

    /// Getter for the transactions root
    pub fn transactions_root(&self) -> Bytes32 {
        match self {
            BlockHeader::V1(header) => header.application().transactions_root,
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => header.application().transactions_root,
        }
    }

    /// Getter for the transactions count
    pub fn transactions_count(&self) -> u16 {
        match self {
            BlockHeader::V1(header) => header.application().transactions_count,
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => header.application().transactions_count,
        }
    }

    /// Getter for the message receipt count
    pub fn message_receipt_count(&self) -> u32 {
        match self {
            BlockHeader::V1(header) => header.application().message_receipt_count,
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => header.application().message_receipt_count,
        }
    }

    /// Getter for the message outbox root
    pub fn message_outbox_root(&self) -> Bytes32 {
        match self {
            BlockHeader::V1(header) => header.application().message_outbox_root,
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => header.application().message_outbox_root,
        }
    }

    /// Getter for the event inbox root
    pub fn event_inbox_root(&self) -> Bytes32 {
        match self {
            BlockHeader::V1(header) => header.application().event_inbox_root,
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => header.application().event_inbox_root,
        }
    }

    /// Getter for the transaction ID commitment
    pub fn tx_id_commitment(&self) -> Option<Bytes32> {
        match self {
            BlockHeader::V1(header) => header.tx_id_commitment(),
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => header.tx_id_commitment(),
        }
    }

    /// Alias the application header into an empty one.
    pub fn as_empty_application_header(&self) -> ApplicationHeader<Empty> {
        match self {
            BlockHeader::V1(header) => ApplicationHeader {
                da_height: header.application().da_height,
                consensus_parameters_version: header
                    .application()
                    .consensus_parameters_version,
                state_transition_bytecode_version: header
                    .application()
                    .state_transition_bytecode_version,
                generated: Empty {},
            },
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => ApplicationHeader {
                da_height: header.application().da_height,
                consensus_parameters_version: header
                    .application()
                    .consensus_parameters_version,
                state_transition_bytecode_version: header
                    .application()
                    .state_transition_bytecode_version,
                generated: Empty {},
            },
        }
    }
}

#[cfg(any(test, feature = "test-helpers"))]
impl BlockHeader {
    /// Set the time for the header
    pub fn set_time(&mut self, time: Tai64) {
        match self {
            BlockHeader::V1(header) => header.set_time(time),
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => header.set_time(time),
        }
    }

    /// Set the da height for the header
    pub fn set_da_height(&mut self, da_height: DaBlockHeight) {
        match self {
            BlockHeader::V1(header) => header.set_da_height(da_height),
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => header.set_da_height(da_height),
        }
    }

    /// Set the consensus header
    pub fn set_consensus_header(
        &mut self,
        consensus: ConsensusHeader<GeneratedConsensusFields>,
    ) {
        match self {
            BlockHeader::V1(header) => header.set_consensus_header(consensus),
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => header.set_consensus_header(consensus),
        }
    }

    /// Set the previous root for the header
    pub fn set_previous_root(&mut self, root: Bytes32) {
        match self {
            BlockHeader::V1(header) => header.set_previous_root(root),
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => header.set_previous_root(root),
        }
    }

    /// Set the block height for the header
    pub fn set_block_height(&mut self, height: BlockHeight) {
        match self {
            BlockHeader::V1(header) => header.set_block_height(height),
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => header.set_block_height(height),
        }
    }

    /// Mutable getter for consensus portion of header
    pub fn consensus_mut(&mut self) -> &mut ConsensusHeader<GeneratedConsensusFields> {
        match self {
            BlockHeader::V1(header) => header.consensus_mut(),
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => header.consensus_mut(),
        }
    }

    /// Set the transaction root for the header
    pub fn set_transaction_root(&mut self, root: Bytes32) {
        match self {
            BlockHeader::V1(header) => {
                header.set_transaction_root(root);
            }
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => {
                header.set_transaction_root(root);
            }
        }
    }

    /// Set the consensus parameters version
    pub fn set_consensus_parameters_version(
        &mut self,
        version: ConsensusParametersVersion,
    ) {
        match self {
            BlockHeader::V1(header) => {
                header.set_consensus_parameters_version(version);
            }
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => {
                header.set_consensus_parameters_version(version);
            }
        }
    }

    /// Set the stf version
    pub fn set_stf_version(&mut self, version: StateTransitionBytecodeVersion) {
        match self {
            BlockHeader::V1(header) => {
                header.set_stf_version(version);
            }
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => {
                header.set_stf_version(version);
            }
        }
    }

    /// Set the application hash
    pub fn set_application_hash(&mut self, hash: Bytes32) {
        match self {
            BlockHeader::V1(header) => {
                header.set_application_hash(hash);
            }
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => {
                header.set_application_hash(hash);
            }
        }
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
pub const LATEST_STATE_TRANSITION_VERSION: StateTransitionBytecodeVersion = 28;

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

#[cfg(feature = "fault-proving")]
impl From<&ApplicationHeader<GeneratedApplicationFieldsV2>>
    for ApplicationHeader<GeneratedApplicationFieldsV1>
{
    fn from(value: &ApplicationHeader<GeneratedApplicationFieldsV2>) -> Self {
        Self {
            da_height: value.da_height,
            consensus_parameters_version: value.consensus_parameters_version,
            state_transition_bytecode_version: value.state_transition_bytecode_version,
            generated: GeneratedApplicationFieldsV1 {
                transactions_count: value.transactions_count,
                message_receipt_count: value.message_receipt_count,
                transactions_root: value.transactions_root,
                message_outbox_root: value.message_outbox_root,
                event_inbox_root: value.event_inbox_root,
            },
        }
    }
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
        match () {
            #[cfg(feature = "fault-proving")]
            () => {
                let mut default = BlockHeaderV2::default();
                default.recalculate_metadata();

                BlockHeader::V2(default)
            }
            #[cfg(not(feature = "fault-proving"))]
            () => {
                let mut default = BlockHeaderV1::default();
                default.recalculate_metadata();

                BlockHeader::V1(default)
            }
        }
    }
}

#[cfg(any(test, feature = "test-helpers"))]
impl BlockHeader {
    /// Creates the block header around the `height` and `time`.
    /// The method should be used only for tests.
    pub fn new_block(height: BlockHeight, time: Tai64) -> Self {
        let mut default = Self::default();
        match default {
            BlockHeader::V1(ref mut header) => {
                header.consensus_mut().height = height;
                header.consensus_mut().time = time;
                header.recalculate_metadata();
            }
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(ref mut header) => {
                header.consensus_mut().height = height;
                header.consensus_mut().time = time;
                header.recalculate_metadata();
            }
        }
        default
    }
}

// Accessors for the consensus header.
impl BlockHeader {
    /// Merkle root of all previous block header hashes.
    pub fn prev_root(&self) -> &Bytes32 {
        match self {
            BlockHeader::V1(header) => &header.consensus().prev_root,
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => &header.consensus().prev_root,
        }
    }
    /// Fuel block height.
    pub fn height(&self) -> &BlockHeight {
        match self {
            BlockHeader::V1(header) => &header.consensus().height,
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => &header.consensus().height,
        }
    }
    /// The block producer time.
    pub fn time(&self) -> Tai64 {
        match self {
            BlockHeader::V1(header) => header.consensus().time,
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => header.consensus().time,
        }
    }
    /// The hash of the application header.
    pub fn application_hash(&self) -> &Bytes32 {
        match self {
            BlockHeader::V1(header) => &header.consensus().application_hash,
            #[cfg(feature = "fault-proving")]
            BlockHeader::V2(header) => &header.consensus().application_hash,
        }
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

        let da_height = header.da_height();
        let consensus_parameters_version = header.consensus_parameters_version();
        let state_transition_bytecode_version =
            header.state_transition_bytecode_version();

        PartialBlockHeader {
            application: ApplicationHeader {
                da_height,
                consensus_parameters_version,
                state_transition_bytecode_version,
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

        #[cfg(not(feature = "fault-proving"))]
        let mut header: BlockHeader = BlockHeader::V1(BlockHeaderV1 {
            application: ApplicationHeader {
                da_height: self.application.da_height,
                consensus_parameters_version: self
                    .application
                    .consensus_parameters_version,
                state_transition_bytecode_version: self
                    .application
                    .state_transition_bytecode_version,
                generated: GeneratedApplicationFieldsV1 {
                    transactions_count: u16::try_from(transactions.len())
                        .map_err(|_| BlockHeaderError::TooManyTransactions)?,
                    message_receipt_count: u32::try_from(outbox_message_ids.len())
                        .map_err(|_| BlockHeaderError::TooManyMessages)?,
                    transactions_root,
                    message_outbox_root,
                    event_inbox_root,
                },
            },
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
        });

        #[cfg(feature = "fault-proving")]
        let mut header: BlockHeader = BlockHeader::V2(BlockHeaderV2 {
            application: ApplicationHeader {
                da_height: self.application.da_height,
                consensus_parameters_version: self
                    .application
                    .consensus_parameters_version,
                state_transition_bytecode_version: self
                    .application
                    .state_transition_bytecode_version,
                generated: GeneratedApplicationFieldsV2 {
                    transactions_count: u16::try_from(transactions.len())
                        .map_err(|_| BlockHeaderError::TooManyTransactions)?,
                    message_receipt_count: u32::try_from(outbox_message_ids.len())
                        .map_err(|_| BlockHeaderError::TooManyMessages)?,
                    transactions_root,
                    message_outbox_root,
                    event_inbox_root,
                    tx_id_commitment: v2::generate_tx_id_commitment(
                        transactions,
                        chain_id,
                    ),
                },
            },
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
        });

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

impl core::ops::Deref for PartialBlockHeader {
    type Target = ApplicationHeader<Empty>;

    fn deref(&self) -> &Self::Target {
        &self.application
    }
}

impl core::ops::Deref for ConsensusHeader<GeneratedConsensusFields> {
    type Target = GeneratedConsensusFields;

    fn deref(&self) -> &Self::Target {
        &self.generated
    }
}

impl core::convert::AsRef<ConsensusHeader<Empty>> for PartialBlockHeader {
    fn as_ref(&self) -> &ConsensusHeader<Empty> {
        &self.consensus
    }
}
