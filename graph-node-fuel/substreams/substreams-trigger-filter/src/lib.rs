#![allow(clippy::not_unsafe_ptr_arg_deref)]

mod pb;

use pb::receipts::v1::BlockAndReceipts;
use substreams_entity_change::pb::entity::EntityChanges;
use substreams_near_core::pb::sf::near::r#type::v1::{
    execution_outcome, receipt::Receipt, Block, IndexerExecutionOutcomeWithReceipt,
};
use trigger_filters::NearFilter;

fn status(outcome: &IndexerExecutionOutcomeWithReceipt) -> Option<&execution_outcome::Status> {
    outcome
        .execution_outcome
        .as_ref()
        .and_then(|o| o.outcome.as_ref())
        .and_then(|o| o.status.as_ref())
}

fn is_success(outcome: &IndexerExecutionOutcomeWithReceipt) -> bool {
    status(outcome)
        .map(|s| {
            use execution_outcome::Status::*;

            match s {
                Unknown(_) | Failure(_) => false,
                SuccessValue(_) | SuccessReceiptId(_) => true,
            }
        })
        .unwrap_or(false)
}

#[substreams::handlers::map]
fn near_filter(params: String, blk: Block) -> Result<BlockAndReceipts, substreams::errors::Error> {
    let mut blk = blk;
    let filter = NearFilter::try_from(params.as_str())?;
    let mut out = BlockAndReceipts::default();

    blk.shards = blk
        .shards
        .into_iter()
        .map(|shard| {
            let mut shard = shard;
            let receipt_execution_outcomes = shard
                .receipt_execution_outcomes
                .into_iter()
                .filter(|outcome| {
                    if !is_success(&outcome) {
                        return false;
                    }

                    let execution_outcome = match outcome.execution_outcome.as_ref() {
                        Some(eo) => eo,
                        None => return false,
                    };

                    let receipt = match outcome.receipt.as_ref() {
                        Some(receipt) => receipt,
                        None => return false,
                    };

                    if !matches!(receipt.receipt, Some(Receipt::Action(_))) {
                        return false;
                    }

                    if !filter.matches(&receipt.receiver_id) {
                        return false;
                    }

                    out.outcome.push(execution_outcome.clone());
                    out.receipt.push(receipt.clone());
                    true
                })
                .collect();
            shard.receipt_execution_outcomes = receipt_execution_outcomes;
            shard
        })
        .collect();

    out.block = Some(blk.clone());

    Ok(out)
}

#[substreams::handlers::map]
fn graph_out(blk: Block) -> Result<EntityChanges, substreams::errors::Error> {
    let mut out = EntityChanges::default();

    let hex = hex::encode(&blk.header.as_ref().unwrap().hash.as_ref().unwrap().bytes);

    out.push_change(
        "Block",
        &hex,
        blk.header.unwrap().height,
        substreams_entity_change::pb::entity::entity_change::Operation::Create,
    );

    Ok(out)
}
