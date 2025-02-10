use crate::{
    blockchain::{
        header::{
            ApplicationHeader,
            BlockHeaderMetadata,
            ConsensusHeader,
            GeneratedApplicationFields,
            GeneratedConsensusFields,
            GetBlockHeaderFields,
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

/// The fault proving header
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(any(test, feature = "test-helpers"), derive(Default))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FaultProvingHeader {
    /// The transaction id commitment
    pub tx_id_commitment: Bytes32,
}

/// A fuel block header that has all the fields generated because it
/// has been executed.
/// differences from V1:
/// - adds the tx_id_commitment field
#[derive(Clone, Debug, derivative::Derivative)]
#[derivative(PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(any(test, feature = "test-helpers"), derive(Default))]
pub struct BlockHeaderV2 {
    /// The application header.
    pub application: ApplicationHeader<GeneratedApplicationFields>,
    /// The consensus header.
    pub consensus: ConsensusHeader<GeneratedConsensusFields>,
    /// The header metadata calculated during creation.
    /// The field is private to enforce the use of the [`PartialBlockHeader::generate`] method.
    #[cfg_attr(feature = "serde", serde(skip))]
    #[derivative(PartialEq = "ignore")]
    pub(crate) metadata: Option<BlockHeaderMetadata>,
    /// fault proving relevant data
    pub fault_proving: FaultProvingHeader,
}

impl GetBlockHeaderFields for BlockHeaderV2 {
    fn consensus(&self) -> &ConsensusHeader<GeneratedConsensusFields> {
        &self.consensus
    }

    fn application(&self) -> &ApplicationHeader<GeneratedApplicationFields> {
        &self.application
    }

    fn metadata(&self) -> &Option<BlockHeaderMetadata> {
        &self.metadata
    }

    fn recalculate_metadata(&mut self) {
        let application_hash = self.application().hash();
        self.consensus.generated.application_hash = application_hash;
        let id = self.hash();
        self.metadata = Some(BlockHeaderMetadata { id });
    }

    fn hash(&self) -> BlockId {
        debug_assert_eq!(&self.consensus.application_hash, &self.application().hash());
        // This internally hashes the hash of the application header.
        let consensus_header_hash = self.consensus().hash();
        let mut hasher = fuel_crypto::Hasher::default();
        hasher.input::<&Bytes32>(consensus_header_hash.as_ref());
        hasher.input(self.fault_proving.tx_id_commitment.as_ref());
        hasher.digest().into()
    }

    fn tx_id_commitment(&self) -> Option<Bytes32> {
        Some(self.fault_proving.tx_id_commitment)
    }
}

#[cfg(any(test, feature = "test-helpers"))]
impl crate::blockchain::header::BlockHeaderDataTestHelpers for BlockHeaderV2 {
    fn consensus_mut(&mut self) -> &mut ConsensusHeader<GeneratedConsensusFields> {
        &mut self.consensus
    }

    fn set_consensus_header(
        &mut self,
        consensus: ConsensusHeader<GeneratedConsensusFields>,
    ) {
        self.consensus = consensus;
    }

    fn application_mut(&mut self) -> &mut ApplicationHeader<GeneratedApplicationFields> {
        &mut self.application
    }

    fn set_application_header(
        &mut self,
        application: ApplicationHeader<GeneratedApplicationFields>,
    ) {
        self.application = application;
    }

    fn set_block_height(&mut self, height: crate::fuel_types::BlockHeight) {
        self.consensus_mut().height = height;
        self.recalculate_metadata();
    }

    fn set_previous_root(&mut self, root: crate::fuel_tx::Bytes32) {
        self.consensus_mut().prev_root = root;
        self.recalculate_metadata();
    }

    fn set_time(&mut self, time: tai64::Tai64) {
        self.consensus_mut().time = time;
        self.recalculate_metadata();
    }

    fn set_transaction_root(&mut self, root: crate::fuel_tx::Bytes32) {
        self.application_mut().generated.transactions_root = root;
        self.recalculate_metadata();
    }

    fn set_da_height(&mut self, da_height: crate::blockchain::primitives::DaBlockHeight) {
        self.application_mut().da_height = da_height;
        self.recalculate_metadata();
    }
}
