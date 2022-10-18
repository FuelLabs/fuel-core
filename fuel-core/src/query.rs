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
pub trait OutputProofData {
    fn receipts(&self, transaction_id: &Bytes32) -> Result<Vec<Receipt>, KvStoreError>;
    fn transaction(
        &self,
        transaction_id: &Bytes32,
    ) -> Result<Option<Transaction>, KvStoreError>;
    fn transaction_status(
        &self,
        transaction_id: &Bytes32,
    ) -> Result<Option<TransactionStatus>, KvStoreError>;
    fn transactions_on_block(
        &self,
        block_id: &Bytes32,
    ) -> Result<Vec<Bytes32>, KvStoreError>;
    fn message(
        &self,
        message_id: &MessageId,
    ) -> Result<Option<fuel_core_interfaces::model::Message>, KvStoreError>;
    fn signature(
        &self,
        block_id: &Bytes32,
    ) -> Result<Option<fuel_crypto::Signature>, KvStoreError>;
    fn block(&self, block_id: &Bytes32) -> Result<Option<FuelBlockDb>, KvStoreError>;
}

pub async fn output_proof(
    data: &(dyn OutputProofData + Send + Sync),
    transaction_id: Bytes32,
    message_id: MessageId,
) -> Result<Option<OutputProof>, KvStoreError> {
    if !data.receipts(&transaction_id)?.into_iter().any(
        |r| matches!(r, Receipt::MessageOut { message_id: id, .. } if id == message_id),
    ) {
        return Ok(None)
    }
    let block_id = data
        .transaction_status(&transaction_id)?
        .and_then(|status| match status {
            TransactionStatus::Failed { block_id, .. }
            | TransactionStatus::Success { block_id, .. } => Some(block_id),
            TransactionStatus::Submitted { .. } => None,
        });
    let block_id = match block_id {
        Some(b) => b,
        None => return Ok(None),
    };
    let mut message_found = false;
    let leaves = data
        .transactions_on_block(&block_id)?
        .into_iter()
        .filter_map(|transaction_id| {
            // TODO: get this from the block header when it is available.
            match data.transaction(&transaction_id) {
                Ok(transaction) => transaction?
                    .outputs()
                    .iter()
                    .any(Output::is_message)
                    .then(|| data.receipts(&transaction_id)),
                Err(e) => Some(Err(e)),
            }
        })
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
                let iter: Box<dyn Iterator<Item = _>> = Box::new(std::iter::once(Err(e)));
                iter
            }
        })
        .take_while(|id| {
            let message_not_found = !message_found;
            if let Ok(id) = id {
                message_found = *id == message_id;
            }
            message_not_found
        })
        .enumerate();
    let mut tree = fuel_merkle::binary::in_memory::MerkleTree::new();
    let mut proof_index = 0;
    for (index, message_id) in leaves {
        tree.push(message_id?.as_ref());
        proof_index = index;
    }
    if message_found {
        let proof = match tree.prove(proof_index as u64) {
            Some(t) => t,
            None => return Ok(None),
        };
        let message = match data.message(&message_id)? {
            Some(t) => t,
            None => return Ok(None),
        };
        let signature = match data.signature(&block_id)? {
            Some(t) => t,
            None => return Ok(None),
        };
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
