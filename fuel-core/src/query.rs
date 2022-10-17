use fuel_core_interfaces::{
    common::{
        fuel_crypto,
        fuel_merkle,
        fuel_types::MessageId,
        prelude::*,
    },
    model::{
        FuelBlockDb,
        OutputProof,
    },
};

use crate::tx_pool::TransactionStatus;

#[cfg(test)]
mod test;

#[cfg_attr(test, mockall::automock)]
pub trait OutputProofData {
    fn receipts(&self, transaction_id: &Bytes32) -> Vec<Receipt>;
    fn transaction(&self, transaction_id: &Bytes32) -> Option<Transaction>;
    fn transaction_status(&self, transaction_id: &Bytes32) -> Option<TransactionStatus>;
    fn transactions_on_block(&self, block_id: &Bytes32) -> Vec<Bytes32>;
    fn message(
        &self,
        message_id: &MessageId,
    ) -> Option<fuel_core_interfaces::model::Message>;
    fn signature(&self, block_id: &Bytes32) -> Option<fuel_crypto::Signature>;
    fn block(&self, block_id: &Bytes32) -> Option<FuelBlockDb>;
}

pub async fn output_proof(
    data: &(dyn OutputProofData + Send + Sync),
    transaction_id: Bytes32,
    message_id: MessageId,
) -> Option<OutputProof> {
    data.receipts(&transaction_id).into_iter().find(
        |r| matches!(r, Receipt::MessageOut { message_id: id, .. } if *id == message_id),
    )?;
    let block_id = data
        .transaction_status(&transaction_id)
        .and_then(|status| match status {
            TransactionStatus::Failed { block_id, .. }
            | TransactionStatus::Success { block_id, .. } => Some(block_id),
            TransactionStatus::Submitted { .. } => None,
        })?;
    let mut message_found = false;
    let leaves = data
        .transactions_on_block(&block_id)
        .into_iter()
        .filter(|transaction_id| {
            // TODO: get this from the block header when it is available.
            data.transaction(transaction_id)
                .map_or(false, |txn| txn.outputs().iter().any(|o| o.is_message()))
        })
        .map(|transaction_id| data.receipts(&transaction_id))
        .flat_map(|receipts| {
            receipts.into_iter().filter_map(|r| match r {
                Receipt::MessageOut { message_id, .. } => Some(message_id),
                _ => None,
            })
        })
        .take_while(|id| {
            let message_not_found = !message_found;
            message_found = *id == message_id;
            message_not_found
        })
        .enumerate();
    let mut tree = fuel_merkle::binary::in_memory::MerkleTree::new();
    let mut proof_index = 0;
    for (index, message_id) in leaves {
        tree.push(message_id.as_ref());
        proof_index = index;
    }
    if message_found {
        let proof = tree.prove(proof_index as u64)?;
        let message = data.message(&message_id)?;
        let signature = data.signature(&block_id)?;
        let block = data.block(&block_id)?;
        Some(OutputProof {
            root: proof.0.into(),
            proof_set: proof.1.into_iter().map(Bytes32::from).collect(),
            message,
            signature,
            block,
        })
    } else {
        None
    }
}
