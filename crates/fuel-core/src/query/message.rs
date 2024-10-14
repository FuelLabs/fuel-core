use crate::fuel_core_graphql_api::{
    database::ReadView,
    IntoApiResult,
};
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IterDirection,
    },
    not_found,
    tables::Messages,
    Error as StorageError,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::block::CompressedBlock,
    entities::relayer::message::{
        MerkleProof,
        Message,
        MessageProof,
        MessageStatus,
    },
    fuel_merkle::binary::in_memory::MerkleTree,
    fuel_tx::{
        input::message::compute_message_id,
        Receipt,
        TxId,
    },
    fuel_types::{
        Address,
        BlockHeight,
        Bytes32,
        MessageId,
        Nonce,
    },
    services::txpool::TransactionStatus,
};
use futures::{
    Stream,
    StreamExt,
    TryStreamExt,
};
use itertools::Itertools;
use std::borrow::Cow;

#[cfg(test)]
mod test;

pub trait MessageQueryData: Send + Sync {
    fn message(&self, message_id: &Nonce) -> StorageResult<Message>;

    fn owned_message_ids(
        &self,
        owner: &Address,
        start_message_id: Option<Nonce>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<Nonce>>;

    fn owned_messages(
        &self,
        owner: &Address,
        start_message_id: Option<Nonce>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<Message>>;

    fn all_messages(
        &self,
        start_message_id: Option<Nonce>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<Message>>;
}

impl ReadView {
    pub fn message(&self, id: &Nonce) -> StorageResult<Message> {
        self.on_chain
            .as_ref()
            .storage::<Messages>()
            .get(id)?
            .ok_or(not_found!(Messages))
            .map(Cow::into_owned)
    }

    pub async fn messages(
        &self,
        ids: Vec<Nonce>,
    ) -> impl Iterator<Item = StorageResult<Message>> + '_ {
        // TODO: Use multiget when it's implemented.
        //  https://github.com/FuelLabs/fuel-core/issues/2344
        let messages = ids.into_iter().map(|id| self.message(&id));
        // Give a chance to other tasks to run.
        tokio::task::yield_now().await;
        messages
    }

    pub fn owned_messages<'a>(
        &'a self,
        owner: &'a Address,
        start_message_id: Option<Nonce>,
        direction: IterDirection,
    ) -> impl Stream<Item = StorageResult<Message>> + 'a {
        self.owned_message_ids(owner, start_message_id, direction)
            .chunks(self.batch_size)
            .map(|chunk| {
                let chunk = chunk.into_iter().try_collect::<_, Vec<_>, _>()?;
                Ok(chunk)
            })
            .try_filter_map(move |chunk| async move {
                let chunk = self.messages(chunk).await;
                Ok::<_, StorageError>(Some(futures::stream::iter(chunk)))
            })
            .try_flatten()
    }
}

/// Trait that specifies all the data required by the output message query.
pub trait MessageProofData {
    /// Get the block.
    fn block(&self, id: &BlockHeight) -> StorageResult<CompressedBlock>;

    /// Return all receipts in the given transaction.
    fn receipts(&self, transaction_id: &TxId) -> StorageResult<Vec<Receipt>>;

    /// Get the status of a transaction.
    fn transaction_status(
        &self,
        transaction_id: &TxId,
    ) -> StorageResult<TransactionStatus>;

    /// Gets the [`MerkleProof`] for the message block at `message_block_height` height
    /// relatively to the commit block where message block <= commit block.
    fn block_history_proof(
        &self,
        message_block_height: &BlockHeight,
        commit_block_height: &BlockHeight,
    ) -> StorageResult<MerkleProof>;
}

impl MessageProofData for ReadView {
    fn block(&self, id: &BlockHeight) -> StorageResult<CompressedBlock> {
        self.block(id)
    }

    fn receipts(&self, transaction_id: &TxId) -> StorageResult<Vec<Receipt>> {
        self.receipts(transaction_id)
    }

    fn transaction_status(
        &self,
        transaction_id: &TxId,
    ) -> StorageResult<TransactionStatus> {
        self.tx_status(transaction_id)
    }

    fn block_history_proof(
        &self,
        message_block_height: &BlockHeight,
        commit_block_height: &BlockHeight,
    ) -> StorageResult<MerkleProof> {
        self.block_history_proof(message_block_height, commit_block_height)
    }
}

/// Generate an output proof.
// TODO: Do we want to return `Option` here?
pub fn message_proof<T: MessageProofData + ?Sized>(
    database: &T,
    transaction_id: Bytes32,
    desired_nonce: Nonce,
    commit_block_height: BlockHeight,
) -> StorageResult<Option<MessageProof>> {
    // Check if the receipts for this transaction actually contain this message id or exit.
    let receipt = database
        .receipts(&transaction_id)?
        .into_iter()
        .find_map(|r| match r {
            Receipt::MessageOut {
                sender,
                recipient,
                nonce,
                amount,
                data,
                ..
            } if r.nonce() == Some(&desired_nonce) => {
                Some((sender, recipient, nonce, amount, data))
            }
            _ => None,
        });

    let (sender, recipient, nonce, amount, data) = match receipt {
        Some(r) => r,
        None => return Ok(None),
    };
    let data =
        data.ok_or(anyhow::anyhow!("Output message doesn't contain any `data`"))?;

    // Get the block id from the transaction status if it's ready.
    let message_block_height = match database
        .transaction_status(&transaction_id)
        .into_api_result::<TransactionStatus, StorageError>(
    )? {
        Some(TransactionStatus::Success { block_height, .. }) => block_height,
        _ => return Ok(None),
    };

    // Get the message fuel block header.
    let (message_block_header, message_block_txs) = match database
        .block(&message_block_height)
        .into_api_result::<CompressedBlock, StorageError>()?
    {
        Some(t) => t.into_inner(),
        None => return Ok(None),
    };

    let message_id = compute_message_id(&sender, &recipient, &nonce, amount, &data);

    let message_proof =
        match message_receipts_proof(database, message_id, &message_block_txs)? {
            Some(proof) => proof,
            None => return Ok(None),
        };

    // Get the commit fuel block header.
    let commit_block_header = match database
        .block(&commit_block_height)
        .into_api_result::<CompressedBlock, StorageError>()?
    {
        Some(t) => t.into_inner().0,
        None => return Ok(None),
    };

    let block_height = *commit_block_header.height();
    if block_height == 0u32.into() {
        // Cannot look beyond the genesis block
        return Ok(None)
    }
    let verifiable_commit_block_height =
        block_height.pred().expect("We checked the height above");
    let block_proof = database.block_history_proof(
        message_block_header.height(),
        &verifiable_commit_block_height,
    )?;

    Ok(Some(MessageProof {
        message_proof,
        block_proof,
        message_block_header,
        commit_block_header,
        sender,
        recipient,
        nonce,
        amount,
        data,
    }))
}

fn message_receipts_proof<T: MessageProofData + ?Sized>(
    database: &T,
    message_id: MessageId,
    message_block_txs: &[Bytes32],
) -> StorageResult<Option<MerkleProof>> {
    // Get the message receipts from the block.
    let leaves: Vec<Vec<Receipt>> = message_block_txs
        .iter()
        .map(|id| database.receipts(id))
        .filter_map(|result| result.into_api_result::<_, StorageError>().transpose())
        .try_collect()?;
    let leaves = leaves.into_iter()
        // Flatten the receipts after filtering on output messages
        // and mapping to message ids.
        .flat_map(|receipts|
            receipts.into_iter().filter_map(|r| r.message_id()));

    // Build the merkle proof from the above iterator.
    let mut tree = MerkleTree::new();

    let mut proof_index = None;

    for (index, id) in leaves.enumerate() {
        // Check if this is the message id being proved.
        if message_id == id {
            // Save the index of this message to use as the proof index.
            proof_index = Some(index as u64);
        }

        // Build the merkle tree.
        tree.push(id.as_ref());
    }

    // If we found the leaf proof index then return the proof.
    match proof_index {
        Some(proof_index) => {
            // Generate the actual merkle proof.
            match tree.prove(proof_index) {
                Some((_, proof_set)) => Ok(Some(MerkleProof {
                    proof_set,
                    proof_index,
                })),
                None => Ok(None),
            }
        }
        None => Ok(None),
    }
}

pub fn message_status(
    database: &ReadView,
    message_nonce: Nonce,
) -> StorageResult<MessageStatus> {
    if database.message_is_spent(&message_nonce)? {
        Ok(MessageStatus::spent())
    } else if database.message_exists(&message_nonce)? {
        Ok(MessageStatus::unspent())
    } else {
        Ok(MessageStatus::not_found())
    }
}
