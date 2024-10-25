use crate::fuel_core_graphql_api::database::ReadView;
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
    ) -> impl Stream<Item = StorageResult<Message>> {
        let ids = Box::new(ids.iter());
        let messages: Vec<_> = self
            .on_chain
            .as_ref()
            .storage::<Messages>()
            .get_multi(Box::new(ids))
            .map(|res| {
                res.and_then(|opt| opt.ok_or(not_found!(Messages)).map(Cow::into_owned))
            })
            .collect();

        //// TODO: Use multiget when it's implemented.
        ////  https://github.com/FuelLabs/fuel-core/issues/2344
        // let messages = ids.into_iter().map(|id| self.message(&id));
        //// Give a chance to other tasks to run.
        tokio::task::yield_now().await;
        futures::stream::iter(messages)
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
                Ok::<_, StorageError>(Some(chunk))
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
pub fn message_proof<T: MessageProofData + ?Sized>(
    database: &T,
    transaction_id: Bytes32,
    desired_nonce: Nonce,
    commit_block_height: BlockHeight,
) -> StorageResult<MessageProof> {
    // Check if the receipts for this transaction actually contain this nonce or exit.
    let (sender, recipient, nonce, amount, data) = database
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
        })
        .ok_or::<StorageError>(
            anyhow::anyhow!("Desired `nonce` missing in transaction receipts").into(),
        )?;

    let Some(data) = data else {
        return Err(anyhow::anyhow!("Output message doesn't contain any `data`").into())
    };

    // Get the block id from the transaction status if it's ready.
    let message_block_height = match database.transaction_status(&transaction_id) {
        Ok(TransactionStatus::Success { block_height, .. }) => block_height,
        Ok(TransactionStatus::Submitted { .. }) => {
            return Err(anyhow::anyhow!(
                "Unable to obtain the message block height. The transaction has not been processed yet"
            )
            .into())
        }
        Ok(TransactionStatus::SqueezedOut { reason }) => {
            return Err(anyhow::anyhow!(
                "Unable to obtain the message block height. The transaction was squeezed out: {reason}"
            )
            .into())
        }
        Ok(TransactionStatus::Failed { .. }) => {
            return Err(anyhow::anyhow!(
                "Unable to obtain the message block height. The transaction failed"
            )
            .into())
        }
        Err(err) => {
            return Err(anyhow::anyhow!(
                "Unable to obtain the message block height: {err}"
            )
            .into())
        }
    };

    // Get the message fuel block header.
    let (message_block_header, message_block_txs) =
        match database.block(&message_block_height) {
            Ok(message_block) => message_block.into_inner(),
            Err(err) => {
                return Err(anyhow::anyhow!(
                    "Unable to get the message block from the database: {err}"
                )
                .into())
            }
        };

    let message_id = compute_message_id(&sender, &recipient, &nonce, amount, &data);

    let message_proof = message_receipts_proof(database, message_id, &message_block_txs)?;

    // Get the commit fuel block header.
    let (commit_block_header, _) = match database.block(&commit_block_height) {
        Ok(commit_block_header) => commit_block_header.into_inner(),
        Err(err) => {
            return Err(anyhow::anyhow!(
                "Unable to get commit block header from database: {err}"
            )
            .into())
        }
    };

    let Some(verifiable_commit_block_height) = commit_block_header.height().pred() else {
        return Err(anyhow::anyhow!(
            "Impossible to generate proof beyond the genesis block"
        )
        .into())
    };
    let block_proof = database.block_history_proof(
        message_block_header.height(),
        &verifiable_commit_block_height,
    )?;

    Ok(MessageProof {
        message_proof,
        block_proof,
        message_block_header,
        commit_block_header,
        sender,
        recipient,
        nonce,
        amount,
        data,
    })
}

fn message_receipts_proof<T: MessageProofData + ?Sized>(
    database: &T,
    message_id: MessageId,
    message_block_txs: &[Bytes32],
) -> StorageResult<MerkleProof> {
    // Get the message receipts from the block.
    let leaves: Vec<Vec<Receipt>> = message_block_txs
        .iter()
        .map(|id| database.receipts(id))
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

    // Check if we found a leaf.
    let Some(proof_index) = proof_index else {
        return Err(anyhow::anyhow!(
            "Unable to find the message receipt in the transaction to generate the proof"
        )
        .into())
    };

    // Get the proof set.
    let Some((_, proof_set)) = tree.prove(proof_index) else {
        return Err(anyhow::anyhow!(
            "Unable to generate the Merkle proof for the message from its receipts"
        )
        .into());
    };

    // Return the proof.
    Ok(MerkleProof {
        proof_set,
        proof_index,
    })
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
