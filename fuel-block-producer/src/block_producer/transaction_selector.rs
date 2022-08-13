use crate::Config;
use fuel_core_interfaces::common::{fuel_tx::CheckedTransaction, fuel_types::Word};

pub fn select_transactions(
    mut includable_txs: Vec<CheckedTransaction>,
    config: &Config,
) -> Vec<CheckedTransaction> {
    // basic selection mode for now. Select all includable txs up until gas limit, sorted by fees.
    //
    // Future improvements to this algorithm may take into account the parallel nature of
    // transactions to maximize throughput.
    let mut used_block_space = Word::MIN;

    // ensure txs are sorted by a fee
    includable_txs.sort_by(|a, b| a.max_fee().cmp(&b.max_fee()));

    includable_txs
        .into_iter()
        .take_while(|tx| {
            let tx_block_space = tx.max_fee();
            let new_used_space = used_block_space.saturating_add(tx_block_space);
            let hit_end = new_used_space > config.max_gas_per_block;
            used_block_space = new_used_space;
            !hit_end
        })
        .collect()
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
