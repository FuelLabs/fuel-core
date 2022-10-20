use fuel_core_interfaces::{
    common::{
        fuel_crypto,
        fuel_merkle,
        fuel_types::MessageId,
        prelude::*,
    },
    db::KvStoreError,
    model::{
        FuelBlockDb,
        OutputProof,
    },
};

use crate::tx_pool::TransactionStatus;

#[cfg(test)]
mod test;

#[cfg_attr(test, mockall::automock)]
/// Trait that specifies all the data required by the output message query.
pub trait OutputProofData {
    /// Return all receipts in the given transaction.
    fn receipts(&self, transaction_id: &Bytes32) -> Result<Vec<Receipt>, KvStoreError>;
    /// Get the transaction.
    fn transaction(
        &self,
        transaction_id: &Bytes32,
    ) -> Result<Option<Transaction>, KvStoreError>;
    /// Get the status of a transaction.
    fn transaction_status(
        &self,
        transaction_id: &Bytes32,
    ) -> Result<Option<TransactionStatus>, KvStoreError>;
    /// Get all transactions on a block.
    fn transactions_on_block(
        &self,
        block_id: &Bytes32,
    ) -> Result<Vec<Bytes32>, KvStoreError>;
    /// Get the message associated with the given message id.
    fn message(
        &self,
        message_id: &MessageId,
    ) -> Result<Option<fuel_core_interfaces::model::Message>, KvStoreError>;
    /// Get the signature of a fuel block.
    fn signature(
        &self,
        block_id: &Bytes32,
    ) -> Result<Option<fuel_crypto::Signature>, KvStoreError>;
    /// Get the fuel block.
    fn block(&self, block_id: &Bytes32) -> Result<Option<FuelBlockDb>, KvStoreError>;
}

/// Generate an output proof.
pub async fn output_proof(
    data: &(dyn OutputProofData + Send + Sync),
    transaction_id: Bytes32,
    message_id: MessageId,
) -> Result<Option<OutputProof>, KvStoreError> {
    // Check if the receipts for this transaction actually contain this message id or exit.
    if !data.receipts(&transaction_id)?.into_iter().any(
        |r| matches!(r, Receipt::MessageOut { message_id: id, .. } if id == message_id),
    ) {
        return Ok(None)
    }

    // Get the block id from the transaction status if it's ready.
    let block_id = data
        .transaction_status(&transaction_id)?
        .and_then(|status| match status {
            TransactionStatus::Failed { block_id, .. }
            | TransactionStatus::Success { block_id, .. } => Some(block_id),
            TransactionStatus::Submitted { .. } => None,
        });

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
            Ok(transaction) => transaction?
                .outputs()
                .iter()
                .any(Output::is_message)
                .then(|| data.receipts(&transaction_id)),
            Err(e) => Some(Err(e)),
        })
        // Flatten the receipts after filtering on output messages
        // and mapping to message ids.
        .flat_map(|receipts| match receipts {
            Ok(receipts) => {
                let iter: Box<dyn Iterator<Item = _>> =
                    Box::new(receipts.into_iter().filter_map(|r| match r {
                        Receipt::MessageOut { message_id, .. } => Some(Ok(message_id)),
                        _ => None,
                    }));
                iter
            }
            Err(e) => {
                // Boxing is required because of the different iterator types
                // returned depending on the error case.
                let iter: Box<dyn Iterator<Item = _>> = Box::new(std::iter::once(Err(e)));
                iter
            }
        });

    // Build the merkle proof from the above iterator.
    let mut tree = fuel_merkle::binary::in_memory::MerkleTree::new();

    let mut proof_index = 0;
    // Track if we have found the message we are looking for.
    let mut message_found = false;

    for id in leaves {
        let id = id?;
        // If we haven't found the message yet then check this id.
        if !message_found {
            message_found = message_id == id;
            // Bump the index until we find the id.
            proof_index += 1;
        }
        // Build the merkle tree.
        tree.push(id.as_ref());
    }

    // If we found the proof then return the proof.
    if message_found {
        // Generate the actual merkle proof.
        let proof = match tree.prove(proof_index as u64) {
            Some(t) => t,
            None => return Ok(None),
        };

        // Get the message.
        let message = match data.message(&message_id)? {
            Some(t) => t,
            None => return Ok(None),
        };

        // Get the signature.
        let signature = match data.signature(&block_id)? {
            Some(t) => t,
            None => return Ok(None),
        };

        // Get the fuel block.
        let block = match data.block(&block_id)? {
            Some(t) => t,
            None => return Ok(None),
        };

        Ok(Some(OutputProof {
            root: proof.0.into(),
            proof_set: proof.1.into_iter().map(Bytes32::from).collect(),
            message,
            signature,
            block,
        }))
    } else {
        Ok(None)
    }
}
