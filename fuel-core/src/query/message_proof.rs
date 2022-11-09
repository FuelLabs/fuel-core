use fuel_core_interfaces::{
    common::{
        fuel_crypto,
        fuel_merkle,
        fuel_tx::field::Outputs,
        fuel_types::MessageId,
        prelude::*,
    },
    db::KvStoreError,
    model::{
        FuelBlockDb,
        MessageProof,
    },
};

use crate::tx_pool::TransactionStatus;

#[cfg(test)]
mod test;

#[cfg_attr(test, mockall::automock)]
/// Trait that specifies all the data required by the output message query.
pub trait MessageProofData {
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
    /// Get the signature of a fuel block.
    fn signature(
        &self,
        block_id: &Bytes32,
    ) -> Result<Option<fuel_crypto::Signature>, KvStoreError>;
    /// Get the fuel block.
    fn block(&self, block_id: &Bytes32) -> Result<Option<FuelBlockDb>, KvStoreError>;
}

/// Generate an output proof.
pub async fn message_proof(
    data: &(dyn MessageProofData + Send + Sync),
    transaction_id: Bytes32,
    message_id: MessageId,
) -> Result<Option<MessageProof>, KvStoreError> {
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
    let block_id = data
        .transaction_status(&transaction_id)?
        .and_then(|status| match status {
            TransactionStatus::Failed { block_id, .. }
            | TransactionStatus::Success { block_id, .. } => Some(block_id),
            TransactionStatus::Submitted { .. }
            | TransactionStatus::SqueezedOut { .. } => None,
        });

    // Exit if the status doesn't exist or is not ready.
    let block_id = match block_id {
        Some(b) => b,
        None => return Ok(None),
    };

    // Get the message ids in the same order as the transactions.
    let leaves = data
        .transactions_on_block(&block_id.into())?
        .into_iter()
        // Filter out transactions that contain no messages 
        // and get the receipts for the rest.
        .filter_map(|transaction_id| match data.transaction(&transaction_id) {
            Ok(transaction) => transaction.as_ref().map(|tx| match tx {
                Transaction::Script(script) => script.outputs(),
                Transaction::Create(create) => create.outputs(),
                Transaction::Mint(mint) => mint.outputs(),
            })?
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
            let signature = match data.signature(&block_id.into())? {
                Some(t) => t,
                None => return Ok(None),
            };

            // Get the fuel block.
            let header = match data.block(&block_id.into())? {
                Some(t) => t.header,
                None => return Ok(None),
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
