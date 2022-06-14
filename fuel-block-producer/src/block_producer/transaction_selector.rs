use crate::Config;
use anyhow::Result;
use fuel_core_interfaces::common::{fuel_tx::Transaction, fuel_types::Word};
use std::{cmp::Ordering, sync::Arc};

pub fn select_transactions(
    mut includable_txs: Vec<Arc<Transaction>>,
    config: &Config,
) -> Result<Vec<Transaction>> {
    // basic selection mode for now. Select all includable txs up until gas limit, sorted by fees.
    //
    // Future improvements to this algorithm may take into account the parallel nature of
    // transactions to maximize throughput.
    let mut used_block_space = Word::MIN;

    // sort txs by fee
    includable_txs.sort_by(|a, b| compare_fee(a, b));

    Ok(includable_txs
        .into_iter()
        .take_while(|tx| {
            let tx_block_space = (tx.metered_bytes_size() as Word) + tx.gas_limit();
            let new_used_space = used_block_space.saturating_add(tx_block_space);
            let hit_end = new_used_space > config.max_gas_per_block;
            used_block_space = new_used_space;
            !hit_end
        })
        .map(|tx| tx.as_ref().clone())
        .collect())
}

fn compare_fee(tx1: &Transaction, tx2: &Transaction) -> Ordering {
    fee(tx1).cmp(&fee(tx2))
}

fn fee(tx: &Transaction) -> Word {
    // TODO: dedupe this fee calculation logic once checked transactions are used
    let bytes = f64::ceil(tx.metered_bytes_size() as f64 * tx.byte_price() as f64) as u64;
    let gas = f64::ceil(tx.gas_limit() as f64 * tx.gas_limit() as f64) as u64;

    bytes + gas
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_uses_highest_gas_txs() {}

    #[test]
    fn selector_sorts_txs_by_fee() {}

    #[test]
    fn selector_doesnt_exceed_mas_gas_per_block() {}
}
