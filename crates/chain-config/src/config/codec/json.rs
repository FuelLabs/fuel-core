mod chain_state;
mod read;
mod write;

pub use chain_state::*;
pub use read::*;

#[cfg(test)]
mod tests {
    use crate::{
        config::{
            contract_balance::ContractBalance,
            contract_state::ContractState,
        },
        CoinConfig,
        ContractConfig,
        MessageConfig,
        StateConfig,
    };
    use itertools::Itertools;

    use super::*;

    fn state_config(
        amount: usize,
        per_contract: usize,
        rng: &mut impl rand::Rng,
    ) -> StateConfig {
        StateConfig {
            coins: 
                Some(std::iter::repeat_with(|| CoinConfig::random(rng))
                    .take(amount)
                    .collect()),
            messages:
                Some(std::iter::repeat_with(|| MessageConfig::random(rng))
                    .take(amount)
                    .collect()),
            contracts:
                Some(std::iter::repeat_with(|| ContractConfig::random(rng))
                    .take(amount)
                    .collect()),
            contract_state: std::iter::repeat_with(|| ContractState::random(rng))
                .chunks(per_contract)
                .into_iter()
                .map(|chunk| chunk.collect_vec())
                .take(amount)
                .collect(),
            contract_balance: std::iter::repeat_with(|| ContractBalance::random(rng))
                .chunks(per_contract)
                .into_iter()
                .map(|chunk| chunk.collect_vec())
                .take(amount)
                .collect(),
        }
    }

    #[cfg(feature = "random")]
    #[test]
    fn reads_coins_in_correct_batch_sizes() {
        use crate::BatchGenerator;
        let state = state_config(100, 100, &mut rand::thread_rng());
        let mut reader = JsonBatchReader::<CoinConfig>::from_state(state.clone(), 50);

        let batch_1 = reader.next_batch().unwrap().unwrap();
        let batch_2 = reader.next_batch().unwrap().unwrap();
        let batch_3 = reader.next_batch();

        assert_eq!(batch_1.data, state.coins.clone().unwrap()[..50]);
        assert_eq!(batch_2.data, state.coins.unwrap()[50..]);
        assert!(batch_3.is_none());
    }

    // #[cfg(feature = "random")]
    // #[test]
    // fn encodes_and_decodes_coins() {
    //     use std::iter::repeat_with;
    //     use crate::{json::write::JsonBatchWriter, BatchWriter};

    //     let coins = repeat_with(|| CoinConfig::random(&mut rand::thread_rng()))
    //         .take(100)
    //         .collect_vec();
    //     let state = StateConfig {
    //         coins,
    //         ..Default::default()
    //     };

    //     let mut writer = JsonBatchWriter::new();
    //     writer.write_batch(coins.clone()).unwrap();

    //     // then
    //     let reader =
    //         ParquetBatchReader::<_, CoinConfig>::new(Bytes::from(writer.into_inner()))
    //             .unwrap();

    //     let decoded_codes = reader.into_iter().collect::<Result<Vec<_>, _>>().unwrap();

    //     assert_eq!(
    //         vec![Batch {
    //             data: coins,
    //             group_index: 0
    //         }],
    //         decoded_codes
    //     );
    // }
}
