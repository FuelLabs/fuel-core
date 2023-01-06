use crate::{
    fuel_core_graphql_api::service::DatabaseTemp,
    query::{
        BlockQueryContext,
        TransactionQueryContext,
    },
    state::IterDirection,
};
use fuel_core_storage::{
    iter::IntoBoxedIter,
    not_found,
    tables::Messages,
    Error as StorageError,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        consensus::Consensus,
        primitives::BlockId,
    },
    entities::message::{
        Message,
        MessageProof,
    },
    fuel_crypto::Signature,
    fuel_merkle,
    fuel_tx::{
        field::Outputs,
        Output,
        Receipt,
        Transaction,
        TxId,
    },
    fuel_types::{
        Address,
        Bytes32,
        MessageId,
    },
    services::txpool::TransactionStatus,
};
use std::borrow::Cow;

#[cfg(test)]
mod test;

pub struct MessageQueryContext<'a>(pub &'a DatabaseTemp);

impl MessageQueryContext<'_> {
    pub fn message(&self, message_id: &MessageId) -> StorageResult<Message> {
        self.0
            .as_ref()
            .storage::<Messages>()
            .get(message_id)?
            .ok_or(not_found!(Messages))
            .map(Cow::into_owned)
    }

    pub fn owned_message_ids(
        &self,
        owner: &Address,
        start_message_id: Option<MessageId>,
        direction: IterDirection,
    ) -> impl Iterator<Item = StorageResult<MessageId>> + '_ {
        self.0.owned_message_ids(owner, start_message_id, direction)
    }

    pub fn owned_messages(
        &self,
        owner: &Address,
        start_message_id: Option<MessageId>,
        direction: IterDirection,
    ) -> impl Iterator<Item = StorageResult<Message>> + '_ {
        self.owned_message_ids(owner, start_message_id, direction)
            .map(|result| result.and_then(|id| self.message(&id)))
    }

    pub fn all_messages(
        &self,
        start_message_id: Option<MessageId>,
        direction: IterDirection,
    ) -> impl Iterator<Item = StorageResult<Message>> + '_ {
        self.0.all_messages(start_message_id, direction)
    }
}

#[cfg_attr(test, mockall::automock)]
/// Trait that specifies all the data required by the output message query.
pub trait MessageProofData {
    /// Return all receipts in the given transaction.
    fn receipts(&self, transaction_id: &TxId) -> StorageResult<Vec<Receipt>>;
    /// Get the transaction.
    fn transaction(&self, transaction_id: &TxId) -> StorageResult<Transaction>;
    /// Get the status of a transaction.
    fn transaction_status(
        &self,
        transaction_id: &TxId,
    ) -> StorageResult<TransactionStatus>;
    /// Get all transactions on a block.
    fn transactions_on_block(&self, block_id: &BlockId) -> StorageResult<Vec<Bytes32>>;
    /// Get the signature of a fuel block.
    fn signature(&self, block_id: &BlockId) -> StorageResult<Signature>;
    /// Get the fuel block.
    fn block(&self, block_id: &BlockId) -> StorageResult<CompressedBlock>;
}

impl MessageProofData for MessageQueryContext<'_> {
    fn receipts(&self, transaction_id: &TxId) -> StorageResult<Vec<Receipt>> {
        TransactionQueryContext(self.0).receipts(transaction_id)
    }

    fn transaction(&self, transaction_id: &TxId) -> StorageResult<Transaction> {
        TransactionQueryContext(self.0).transaction(transaction_id)
    }

    fn transaction_status(
        &self,
        transaction_id: &TxId,
    ) -> StorageResult<TransactionStatus> {
        TransactionQueryContext(self.0).status(transaction_id)
    }

    fn transactions_on_block(&self, block_id: &BlockId) -> StorageResult<Vec<TxId>> {
        self.block(block_id).map(|block| block.into_inner().1)
    }

    fn signature(&self, block_id: &BlockId) -> StorageResult<Signature> {
        let consensus = BlockQueryContext(self.0).consensus(block_id)?;
        match consensus {
            // TODO: https://github.com/FuelLabs/fuel-core/issues/816
            Consensus::Genesis(_) => Ok(Default::default()),
            Consensus::PoA(c) => Ok(c.signature),
        }
    }

    fn block(&self, block_id: &BlockId) -> StorageResult<CompressedBlock> {
        BlockQueryContext(self.0).block(block_id)
    }
}

/// Generate an output proof.
// TODO: Do we want to return `Option` here?
pub fn message_proof<Data: MessageProofData>(
    data: &Data,
    transaction_id: Bytes32,
    message_id: MessageId,
) -> StorageResult<Option<MessageProof>> {
    // Check if the receipts for this transaction actually contain this message id or exit.
    let receipt = data
        .receipts(&transaction_id)?
        .into_iter()
        .find_map(|r| match r {
            Receipt::MessageOut {
                message_id: id,
                sender,
                recipient,
                nonce,
                amount,
                data: message_data,
                ..
            } if id == message_id => {
                Some((sender, recipient, nonce, amount, message_data))
            }
            _ => None,
        });

    let (sender, recipient, nonce, amount, message_data) = match receipt {
        Some(r) => r,
        None => return Ok(None),
    };

    // Get the block id from the transaction status if it's ready.
    let block_id = match data.transaction_status(&transaction_id) {
        Ok(status) => match status {
            TransactionStatus::Failed { block_id, .. }
            | TransactionStatus::Success { block_id, .. } => Some(block_id),
            TransactionStatus::Submitted { .. }
            | TransactionStatus::SqueezedOut { .. } => None,
        },
        Err(StorageError::NotFound(_, _)) => return Ok(None),
        Err(err) => return Err(err),
    };

    // Exit if the status doesn't exist or is not ready.
    let block_id = match block_id {
        Some(b) => b,
        None => return Ok(None),
    };

    // Get the message ids in the same order as the transactions.
    let leaves = data
        .transactions_on_block(&block_id)?
        .into_iter()
        // Filter out transactions that contain no messages 
        // and get the receipts for the rest.
        .filter_map(|transaction_id| match data.transaction(&transaction_id) {
            Ok(transaction) => match &transaction {
                Transaction::Script(script) => script.outputs(),
                Transaction::Create(create) => create.outputs(),
                Transaction::Mint(mint) => mint.outputs(),
            }
                .iter()
                .any(Output::is_message)
                .then(|| data.receipts(&transaction_id)),
            Err(StorageError::NotFound(_, _)) => None,
            Err(e) => Some(Err(e)),
        })
        // Flatten the receipts after filtering on output messages
        // and mapping to message ids.
        .flat_map(|receipts| match receipts {
            Ok(receipts) => {
                receipts.into_iter().filter_map(|r| match r {
                        Receipt::MessageOut { message_id, .. } => Some(Ok(message_id)),
                        _ => None,
                    }).into_boxed()
            }
            Err(e) => {
                // Boxing is required because of the different iterator types
                // returned depending on the error case.
                std::iter::once(Err(e)).into_boxed()
            }
        }).enumerate();

    // Build the merkle proof from the above iterator.
    let mut tree = fuel_merkle::binary::in_memory::MerkleTree::new();

    let mut proof_index = None;

    for (index, id) in leaves {
        let id = id?;

        // Check id this is the message.
        if message_id == id {
            // Save the index of this message to use as the proof index.
            proof_index = Some(index);
        }

        // Build the merkle tree.
        tree.push(id.as_ref());
    }

    // If we found the leaf proof index then return the proof.
    match proof_index {
        Some(proof_index) => {
            // Generate the actual merkle proof.
            let proof = match tree.prove(proof_index as u64) {
                Some(t) => t,
                None => return Ok(None),
            };

            // Get the signature.
            let signature = match data.signature(&block_id) {
                Ok(t) => t,
                Err(StorageError::NotFound(_, _)) => return Ok(None),
                Err(err) => return Err(err),
            };

            // Get the fuel block.
            let header = match data.block(&block_id) {
                Ok(t) => t.into_inner().0,
                Err(StorageError::NotFound(_, _)) => return Ok(None),
                Err(err) => return Err(err),
            };

            if *header.output_messages_root != proof.0 {
                // This is bad as it means there's a bug in our prove code.
                tracing::error!(
                    "block header {:?} root doesn't match generated proof root {:?}",
                    header,
                    proof
                );
                return Ok(None)
            }

            Ok(Some(MessageProof {
                proof_set: proof.1.into_iter().map(Bytes32::from).collect(),
                proof_index: proof_index as u64,
                signature,
                header,
                sender,
                recipient,
                nonce,
                amount,
                data: message_data,
            }))
        }
        None => Ok(None),
    }
}
