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
        .filter_map(|id| match database.transaction_status(id) {
            Ok(TransactionStatus::Success { receipts, .. }) => Some(Ok(receipts)),
            Ok(_) => None,
            Err(err) => Some(Err(err)),
        })
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use fuel_core_storage::not_found;
    use fuel_core_types::{
        blockchain::block::CompressedBlock,
        entities::relayer::message::MerkleProof,
        fuel_tx::{
            Address,
            Bytes32,
            Receipt,
            TxId,
        },
        fuel_types::BlockHeight,
        services::txpool::TransactionStatus,
        tai64::Tai64,
    };

    use super::{
        message_receipts_proof,
        MessageProofData,
    };

    pub struct FakeDB {
        pub blocks: HashMap<BlockHeight, CompressedBlock>,
        pub transaction_statuses: HashMap<TxId, TransactionStatus>,
        pub receipts: HashMap<TxId, Vec<Receipt>>,
    }

    impl FakeDB {
        fn new() -> Self {
            Self {
                blocks: HashMap::new(),
                transaction_statuses: HashMap::new(),
                receipts: HashMap::new(),
            }
        }

        fn insert_block(&mut self, block_height: BlockHeight, block: CompressedBlock) {
            self.blocks.insert(block_height, block);
        }

        fn insert_transaction_status(
            &mut self,
            transaction_id: TxId,
            status: TransactionStatus,
        ) {
            self.transaction_statuses.insert(transaction_id, status);
        }

        fn insert_receipts(&mut self, transaction_id: TxId, receipts: Vec<Receipt>) {
            self.receipts.insert(transaction_id, receipts);
        }
    }

    impl MessageProofData for FakeDB {
        fn block(&self, id: &BlockHeight) -> fuel_core_storage::Result<CompressedBlock> {
            self.blocks.get(id).cloned().ok_or(not_found!("Block"))
        }

        fn transaction_status(
            &self,
            transaction_id: &TxId,
        ) -> fuel_core_storage::Result<TransactionStatus> {
            self.transaction_statuses
                .get(transaction_id)
                .cloned()
                .ok_or(not_found!("Transaction status"))
        }

        fn block_history_proof(
            &self,
            _message_block_height: &BlockHeight,
            _commit_block_height: &BlockHeight,
        ) -> fuel_core_storage::Result<MerkleProof> {
            // Unused in current tests
            Ok(MerkleProof::default())
        }

        fn receipts(
            &self,
            transaction_id: &TxId,
        ) -> fuel_core_storage::Result<Vec<Receipt>> {
            self.receipts
                .get(transaction_id)
                .cloned()
                .ok_or(not_found!("Receipts"))
        }
    }

    // Test will try to get the message receipt proof with a block with only valid transactions
    // Then add an invalid transaction and check if the proof is still the same (meaning the invalid transaction was ignored)
    #[test]
    fn test_message_receipts_proof_ignore_failed() {
        // Create a fake database
        let mut database = FakeDB::new();

        // Given
        // Create a block with a valid transaction and receipts
        let mut block = CompressedBlock::default();
        let valid_tx_id = Bytes32::new([1; 32]);
        let mut valid_tx_receipts = vec![];
        for i in 0..100 {
            valid_tx_receipts.push(Receipt::MessageOut {
                sender: Address::default(),
                recipient: Address::default(),
                amount: 0,
                nonce: 0.into(),
                len: 32,
                digest: Bytes32::default(),
                data: Some(vec![i; 32]),
            });
        }
        block.transactions_mut().push(valid_tx_id);
        database.insert_block(1u32.into(), block.clone());
        database.insert_transaction_status(
            valid_tx_id,
            TransactionStatus::Success {
                time: Tai64::UNIX_EPOCH,
                block_height: 1u32.into(),
                receipts: valid_tx_receipts.clone(),
                total_fee: 0,
                total_gas: 0,
                result: None,
            },
        );
        database.insert_receipts(valid_tx_id, valid_tx_receipts.clone());

        // Get the message proof with the valid transaction
        let message_proof_valid_tx = message_receipts_proof(
            &database,
            valid_tx_receipts[0].message_id().unwrap(),
            &[valid_tx_id],
        )
        .unwrap();

        // Add an invalid transaction with receipts to the block
        let invalid_tx_id = Bytes32::new([2; 32]);
        block.transactions_mut().push(invalid_tx_id);
        database.insert_block(1u32.into(), block.clone());
        let mut invalid_tx_receipts = vec![];
        for i in 0..100 {
            invalid_tx_receipts.push(Receipt::MessageOut {
                sender: Address::default(),
                recipient: Address::default(),
                amount: 0,
                nonce: 0.into(),
                len: 33,
                digest: Bytes32::default(),
                data: Some(vec![i; 33]),
            });
        }
        database.insert_transaction_status(
            invalid_tx_id,
            TransactionStatus::Failed {
                time: Tai64::UNIX_EPOCH,
                block_height: 1u32.into(),
                result: None,
                total_fee: 0,
                total_gas: 0,
                receipts: invalid_tx_receipts.clone(),
            },
        );
        database.insert_receipts(invalid_tx_id, invalid_tx_receipts.clone());

        // When
        // Get the message proof with the same message id
        let message_proof_invalid_tx = message_receipts_proof(
            &database,
            valid_tx_receipts[0].message_id().unwrap(),
            &[valid_tx_id, invalid_tx_id],
        )
        .unwrap();

        // Then
        // The proof should be the same because the invalid transaction was ignored
        assert_eq!(message_proof_valid_tx, message_proof_invalid_tx);
    }
}
