mod decoder;
mod encoder;
pub(crate) mod parquet;

pub use decoder::{
    Decoder,
    IntoIter,
};
pub use encoder::Encoder;

use std::fmt::Debug;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Group<T> {
    pub index: usize,
    pub data: Vec<T>,
}
type GroupResult<T> = anyhow::Result<Group<T>>;

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use ::parquet::basic::{
        Compression,
        GzipLevel,
    };

    use crate::{
        config::{
            contract_balance::ContractBalance,
            contract_state::ContractState,
        },
        CoinConfig,
        ContractConfig,
        MessageConfig,
    };

    use itertools::Itertools;

    use super::*;

    #[test]
    fn writes_then_reads_written() {
        let group_size = 100;
        let num_groups = 10;
        let starting_group_index = 3;
        {
            // Json
            let temp_dir = tempfile::tempdir().unwrap();
            let state_encoder = Encoder::json(temp_dir.path());

            let init_decoder = || Decoder::json(temp_dir.path(), group_size).unwrap();

            test_write_read(
                state_encoder,
                init_decoder,
                group_size,
                starting_group_index..num_groups,
            )
        }
        {
            // Parquet
            let temp_dir = tempfile::tempdir().unwrap();
            let compression = Compression::GZIP(GzipLevel::try_new(1).unwrap());
            let state_encoder = Encoder::parquet(temp_dir.path(), compression).unwrap();

            let init_decoder = || Decoder::parquet(temp_dir.path());

            test_write_read(
                state_encoder,
                init_decoder,
                group_size,
                starting_group_index..num_groups,
            )
        }
    }

    fn test_write_read(
        mut encoder: Encoder,
        init_decoder: impl FnOnce() -> Decoder,
        group_size: usize,
        group_range: Range<usize>,
    ) {
        let num_groups = group_range.end;
        let mut rng = rand::thread_rng();
        macro_rules! write_batches {
            ($data_type: ty, $write_method:ident) => {{
                let batches = ::std::iter::repeat_with(|| <$data_type>::random(&mut rng))
                    .chunks(group_size)
                    .into_iter()
                    .map(|chunk| chunk.collect_vec())
                    .enumerate()
                    .map(|(index, data)| Group { index, data })
                    .take(num_groups)
                    .collect_vec();

                for batch in &batches {
                    encoder.$write_method(batch.data.clone()).unwrap();
                }
                batches
            }};
        }

        let coin_batches = write_batches!(CoinConfig, write_coins);
        let message_batches = write_batches!(MessageConfig, write_messages);
        let contract_batches = write_batches!(ContractConfig, write_contracts);
        let contract_state_batches = write_batches!(ContractState, write_contract_state);
        let contract_balance_batches =
            write_batches!(ContractBalance, write_contract_balance);
        encoder.close().unwrap();

        let state_reader = init_decoder();

        let skip_first = group_range.start;
        assert_batches_identical(
            &coin_batches,
            state_reader.coins().unwrap(),
            skip_first,
        );
        assert_batches_identical(
            &message_batches,
            state_reader.messages().unwrap(),
            skip_first,
        );
        assert_batches_identical(
            &contract_batches,
            state_reader.contracts().unwrap(),
            skip_first,
        );
        assert_batches_identical(
            &contract_state_batches,
            state_reader.contract_state().unwrap(),
            skip_first,
        );
        assert_batches_identical(
            &contract_balance_batches,
            state_reader.contract_balance().unwrap(),
            skip_first,
        );
    }

    fn assert_batches_identical<T>(
        original: &[Group<T>],
        read: impl Iterator<Item = Result<Group<T>, anyhow::Error>>,
        skip: usize,
    ) where
        Vec<T>: PartialEq,
        T: PartialEq + std::fmt::Debug,
    {
        pretty_assertions::assert_eq!(
            original[skip..],
            read.skip(skip).collect::<Result<Vec<_>, _>>().unwrap()
        );
    }
}
