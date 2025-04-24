use crate::{
    blockchain::{
        header::{
            ApplicationHeader,
            BlockHeaderMetadata,
            ConsensusHeader,
            GeneratedConsensusFields,
            generate_txns_root,
        },
        primitives::BlockId,
    },
    fuel_crypto,
    fuel_tx::{
        Bytes32,
        Transaction,
        UniqueIdentifier,
    },
    fuel_types::ChainId,
};

use educe::Educe;

#[cfg(feature = "alloc")]
use alloc::vec::Vec;

pub(crate) fn generate_tx_id_commitment(
    transactions: &[Transaction],
    chain_id: &ChainId,
) -> Bytes32 {
    let tx_ids = transactions
        .iter()
        .map(|tx| tx.id(chain_id))
        .collect::<Vec<_>>();
    let mut hasher = fuel_crypto::Hasher::default();
    for tx_id in tx_ids {
        hasher.input(tx_id.as_ref());
    }
    hasher.digest()
}

/// A fuel block header that has all the fields generated because it
/// has been executed.
/// differences from V1:
/// - adds the tx_id_commitment field
#[derive(Clone, Debug, Educe)]
#[educe(PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(any(test, feature = "test-helpers"), derive(Default))]
pub struct BlockHeaderV2 {
    /// The application header.
    pub(crate) application: ApplicationHeader<GeneratedApplicationFieldsV2>,
    /// The consensus header.
    pub(crate) consensus: ConsensusHeader<GeneratedConsensusFields>,
    /// The header metadata calculated during creation.
    /// The field is private to enforce the use of the [`PartialBlockHeader::generate`] method.
    #[cfg_attr(feature = "serde", serde(skip))]
    #[educe(PartialEq(ignore))]
    pub(crate) metadata: Option<BlockHeaderMetadata>,
}

impl BlockHeaderV2 {
    pub(crate) fn consensus(&self) -> &ConsensusHeader<GeneratedConsensusFields> {
        &self.consensus
    }

    /// Returns a reference to the application header.
    pub fn application(&self) -> &ApplicationHeader<GeneratedApplicationFieldsV2> {
        &self.application
    }

    pub(crate) fn metadata(&self) -> &Option<BlockHeaderMetadata> {
        &self.metadata
    }

    pub(crate) fn recalculate_metadata(&mut self) {
        let application_hash = self.application().hash();
        self.consensus.generated.application_hash = application_hash;
        let id = self.hash();
        self.metadata = Some(BlockHeaderMetadata { id });
    }

    pub(crate) fn hash(&self) -> BlockId {
        debug_assert_eq!(&self.consensus.application_hash, &self.application().hash());
        // This internally hashes the hash of the application header.
        self.consensus().hash()
    }

    pub(crate) fn tx_id_commitment(&self) -> Option<Bytes32> {
        Some(self.application().tx_id_commitment)
    }

    pub(crate) fn id(&self) -> BlockId {
        if let Some(metadata) = self.metadata() {
            metadata.id
        } else {
            self.hash()
        }
    }

    pub(crate) fn validate_transactions(&self, transactions: &[Transaction]) -> bool {
        let transactions_root = generate_txns_root(transactions);

        transactions_root == self.application().transactions_root
            && transactions.len() == self.application().transactions_count as usize
    }
}

#[cfg(any(test, feature = "test-helpers"))]
impl BlockHeaderV2 {
    pub(crate) fn consensus_mut(
        &mut self,
    ) -> &mut ConsensusHeader<GeneratedConsensusFields> {
        &mut self.consensus
    }

    pub(crate) fn set_consensus_header(
        &mut self,
        consensus: ConsensusHeader<GeneratedConsensusFields>,
    ) {
        self.consensus = consensus;
    }

    /// Returns a mutable reference to the application header.
    pub fn application_mut(
        &mut self,
    ) -> &mut ApplicationHeader<GeneratedApplicationFieldsV2> {
        &mut self.application
    }

    /// Sets the application header.
    pub fn set_application_header(
        &mut self,
        application: ApplicationHeader<GeneratedApplicationFieldsV2>,
    ) {
        self.application = application;
    }

    pub(crate) fn set_block_height(&mut self, height: crate::fuel_types::BlockHeight) {
        self.consensus_mut().height = height;
        self.recalculate_metadata();
    }

    pub(crate) fn set_previous_root(&mut self, root: crate::fuel_tx::Bytes32) {
        self.consensus_mut().prev_root = root;
        self.recalculate_metadata();
    }

    pub(crate) fn set_time(&mut self, time: tai64::Tai64) {
        self.consensus_mut().time = time;
        self.recalculate_metadata();
    }

    pub(crate) fn set_transaction_root(&mut self, root: crate::fuel_tx::Bytes32) {
        self.application_mut().generated.transactions_root = root;
        self.recalculate_metadata();
    }

    pub(crate) fn set_da_height(
        &mut self,
        da_height: crate::blockchain::primitives::DaBlockHeight,
    ) {
        self.application_mut().da_height = da_height;
        self.recalculate_metadata();
    }

    pub(crate) fn set_consensus_parameters_version(
        &mut self,
        version: super::ConsensusParametersVersion,
    ) {
        self.application_mut().consensus_parameters_version = version;
        self.recalculate_metadata();
    }

    pub(crate) fn set_stf_version(
        &mut self,
        version: super::StateTransitionBytecodeVersion,
    ) {
        self.application_mut().state_transition_bytecode_version = version;
        self.recalculate_metadata();
    }

    pub(crate) fn set_application_hash(&mut self, hash: Bytes32) {
        self.consensus_mut().generated.application_hash = hash;
    }
}

/// Concrete generated application header fields.
/// These are generated once the full block has been run.
/// contains the tx_id_commitment field
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(any(test, feature = "test-helpers"), derive(Default))]
pub struct GeneratedApplicationFieldsV2 {
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
    /// TxID commitment
    pub tx_id_commitment: Bytes32,
}

impl ApplicationHeader<GeneratedApplicationFieldsV2> {
    /// Hash the application header.
    pub fn hash(&self) -> Bytes32 {
        // Order matters and is the same as the spec.
        let mut hasher = crate::fuel_crypto::Hasher::default();
        let Self {
            da_height,
            consensus_parameters_version,
            state_transition_bytecode_version,
            generated:
                GeneratedApplicationFieldsV2 {
                    transactions_count,
                    message_receipt_count,
                    transactions_root,
                    message_outbox_root,
                    event_inbox_root,
                    tx_id_commitment,
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

        // this is the only difference between the two versions
        hasher.input(tx_id_commitment.as_ref());

        hasher.digest()
    }
}

impl core::ops::Deref for ApplicationHeader<GeneratedApplicationFieldsV2> {
    type Target = GeneratedApplicationFieldsV2;

    fn deref(&self) -> &Self::Target {
        &self.generated
    }
}
