use crate::{
    fuel_core_graphql_api::{
        ports::DatabasePort,
        IntoApiResult,
    },
    query::{
        BlockQueryData,
        SimpleBlockData,
        SimpleTransactionData,
        TransactionQueryData,
    },
};
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IntoBoxedIter,
        IterDirection,
    },
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
use itertools::Itertools;
use std::borrow::Cow;

#[cfg(test)]
mod test;

pub trait MessageQueryData: Send + Sync {
    fn message(&self, message_id: &MessageId) -> StorageResult<Message>;

    fn owned_message_ids(
        &self,
        owner: &Address,
        start_message_id: Option<MessageId>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<MessageId>>;

    fn owned_messages(
        &self,
        owner: &Address,
        start_message_id: Option<MessageId>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<Message>>;

    fn all_messages(
        &self,
        start_message_id: Option<MessageId>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<Message>>;
}

impl<D: DatabasePort + ?Sized> MessageQueryData for D {
    fn message(&self, message_id: &MessageId) -> StorageResult<Message> {
        self.storage::<Messages>()
            .get(message_id)?
            .ok_or(not_found!(Messages))
            .map(Cow::into_owned)
    }

    fn owned_message_ids(
        &self,
        owner: &Address,
        start_message_id: Option<MessageId>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<MessageId>> {
        self.owned_message_ids(owner, start_message_id, direction)
    }

    fn owned_messages(
        &self,
        owner: &Address,
        start_message_id: Option<MessageId>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<Message>> {
        self.owned_message_ids(owner, start_message_id, direction)
            .map(|result| result.and_then(|id| self.message(&id)))
            .into_boxed()
    }

    fn all_messages(
        &self,
        start_message_id: Option<MessageId>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<Message>> {
        self.all_messages(start_message_id, direction)
    }
}

/// Trait that specifies all the data required by the output message query.
pub trait MessageProofData:
    Send + Sync + SimpleBlockData + SimpleTransactionData
{
    /// Get the status of a transaction.
    fn transaction_status(
        &self,
        transaction_id: &TxId,
    ) -> StorageResult<TransactionStatus>;
    /// Get all transactions on a block.
    fn transactions_on_block(&self, block_id: &BlockId) -> StorageResult<Vec<Bytes32>>;
    /// Get the signature of a fuel block.
    fn signature(&self, block_id: &BlockId) -> StorageResult<Signature>;
}

impl<D: DatabasePort + ?Sized> MessageProofData for D {
    fn transaction_status(
        &self,
        transaction_id: &TxId,
    ) -> StorageResult<TransactionStatus> {
        self.status(transaction_id)
    }

    fn transactions_on_block(&self, block_id: &BlockId) -> StorageResult<Vec<TxId>> {
        self.block(block_id).map(|block| block.into_inner().1)
    }

    fn signature(&self, block_id: &BlockId) -> StorageResult<Signature> {
        let consensus = self.consensus(block_id)?;
        match consensus {
            // TODO: https://github.com/FuelLabs/fuel-core/issues/816
            Consensus::Genesis(_) => Ok(Default::default()),
            Consensus::PoA(c) => Ok(c.signature),
        }
    }
}

/// Generate an output proof.
// TODO: Do we want to return `Option` here?
pub fn message_proof<T: MessageProofData + ?Sized>(
    data: &T,
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
    let block_id = match data
        .transaction_status(&transaction_id)
        .into_api_result::<TransactionStatus, StorageError>()?
    {
        Some(TransactionStatus::Success { block_id, .. }) => block_id,
        _ => return Ok(None),
    };

    // Get the message ids in the same order as the transactions.
    let leaves: Vec<Vec<Receipt>> = data
        .transactions_on_block(&block_id)?
        .into_iter()
        .filter_map(|id| {
            // Filter out transactions that contain no messages
            // and get the receipts for the rest.
            let result = data.transaction(&id).and_then(|tx| {
                let outputs = match &tx {
                    Transaction::Script(script) => script.outputs(),
                    Transaction::Create(create) => create.outputs(),
                    Transaction::Mint(mint) => mint.outputs(),
                };
                outputs
                    .iter()
                    .any(Output::is_message)
                    .then(|| data.receipts(&id))
                    .transpose()
            });
            result.transpose()
        })
        .filter_map(|result| result.into_api_result::<_, StorageError>().transpose())
        .try_collect()?;

    let leaves = leaves.into_iter()
        // Flatten the receipts after filtering on output messages
        // and mapping to message ids.
        .flat_map(|receipts|
            receipts.into_iter().filter_map(|r| match r {
                    Receipt::MessageOut { message_id, .. } => Some(message_id),
                    _ => None,
                })).enumerate();

    // Build the merkle proof from the above iterator.
    let mut tree = fuel_merkle::binary::in_memory::MerkleTree::new();

    let mut proof_index = None;

    for (index, id) in leaves {
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
            let signature = match data
                .signature(&block_id)
                .into_api_result::<_, StorageError>()?
            {
                Some(t) => t,
                None => return Ok(None),
            };

            // Get the fuel block.
            let header = match data
                .block(&block_id)
                .into_api_result::<CompressedBlock, StorageError>()?
            {
                Some(t) => t.into_inner().0,
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
