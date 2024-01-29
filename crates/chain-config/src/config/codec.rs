mod decoder;
#[cfg(feature = "std")]
mod encoder;
#[cfg(feature = "parquet")]
mod parquet;

pub use decoder::{
    Decoder,
    IntoIter,
};
#[cfg(feature = "std")]
pub use encoder::Encoder;
#[cfg(feature = "parquet")]
pub use encoder::ZstdCompressionLevel;
pub const MAX_GROUP_SIZE: usize = usize::MAX;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Group<T> {
    pub index: usize,
    pub data: Vec<T>,
}
pub(crate) type GroupResult<T> = anyhow::Result<Group<T>>;

#[cfg(all(feature = "std", feature = "random"))]
#[cfg(test)]
mod tests {
    use crate::{
        self as fuel_core_chain_config,
        Group,
    };
    use itertools::Itertools;
    use rand::{
        rngs::StdRng,
        SeedableRng,
    };

    use fuel_core_chain_config::Randomize;

    use super::*;

    #[cfg(feature = "parquet")]
    #[test]
    fn roundtrip_parquet_coins() {
        // given
        let skip_n_groups = 3;
        let temp_dir = tempfile::tempdir().unwrap();

        let mut group_generator = GroupGenerator::new(StdRng::seed_from_u64(0), 100, 10);
        let files = crate::ParquetFiles::snapshot_default(temp_dir.path());
        let mut encoder =
            Encoder::parquet(&files, encoder::ZstdCompressionLevel::Uncompressed)
                .unwrap();

        // when
        let coin_groups =
            group_generator.for_each_group(|group| encoder.write_coins(group));
        encoder.close().unwrap();

        let decoded_coin_groups = Decoder::parquet(files).coins().unwrap().collect_vec();

        // then
        assert_groups_identical(&coin_groups, decoded_coin_groups, skip_n_groups);
    }

    #[cfg(feature = "parquet")]
    #[test]
    fn roundtrip_parquet_messages() {
        // given
        let skip_n_groups = 3;
        let temp_dir = tempfile::tempdir().unwrap();

        let mut group_generator = GroupGenerator::new(StdRng::seed_from_u64(0), 100, 10);
        let files = crate::ParquetFiles::snapshot_default(temp_dir.path());
        let mut encoder =
            Encoder::parquet(&files, encoder::ZstdCompressionLevel::Level1).unwrap();

        // when
        let message_groups =
            group_generator.for_each_group(|group| encoder.write_messages(group));
        encoder.close().unwrap();
        let messages_decoded = Decoder::parquet(files).messages().unwrap().collect_vec();

        // then
        assert_groups_identical(&message_groups, messages_decoded, skip_n_groups);
    }

    #[cfg(feature = "parquet")]
    #[test]
    fn roundtrip_parquet_contracts() {
        // given
        let skip_n_groups = 3;
        let temp_dir = tempfile::tempdir().unwrap();

        let mut group_generator = GroupGenerator::new(StdRng::seed_from_u64(0), 100, 10);
        let files = crate::ParquetFiles::snapshot_default(temp_dir.path());
        let mut encoder =
            Encoder::parquet(&files, encoder::ZstdCompressionLevel::Level1).unwrap();

        // when
        let contract_groups =
            group_generator.for_each_group(|group| encoder.write_contracts(group));
        encoder.close().unwrap();
        let contract_decoded = Decoder::parquet(files).contracts().unwrap().collect_vec();

        // then
        assert_groups_identical(&contract_groups, contract_decoded, skip_n_groups);
    }

    #[cfg(feature = "parquet")]
    #[test]
    fn roundtrip_parquet_contract_state() {
        // given
        let skip_n_groups = 3;
        let temp_dir = tempfile::tempdir().unwrap();

        let mut group_generator = GroupGenerator::new(StdRng::seed_from_u64(0), 100, 10);
        let files = crate::ParquetFiles::snapshot_default(temp_dir.path());
        let mut encoder =
            Encoder::parquet(&files, encoder::ZstdCompressionLevel::Level1).unwrap();

        // when
        let contract_state_groups =
            group_generator.for_each_group(|group| encoder.write_contract_state(group));
        encoder.close().unwrap();
        let decoded_contract_state = Decoder::parquet(files)
            .contract_state()
            .unwrap()
            .collect_vec();

        // then
        assert_groups_identical(
            &contract_state_groups,
            decoded_contract_state,
            skip_n_groups,
        );
    }

    #[cfg(feature = "parquet")]
    #[test]
    fn roundtrip_parquet_contract_balance() {
        // given
        let skip_n_groups = 3;
        let temp_dir = tempfile::tempdir().unwrap();

        let mut group_generator = GroupGenerator::new(StdRng::seed_from_u64(0), 100, 10);
        let files = crate::ParquetFiles::snapshot_default(temp_dir.path());
        let mut encoder =
            Encoder::parquet(&files, encoder::ZstdCompressionLevel::Level1).unwrap();

        // when
        let contract_balance_groups =
            group_generator.for_each_group(|group| encoder.write_contract_balance(group));
        encoder.close().unwrap();

        let decoded_contract_balance = Decoder::parquet(files)
            .contract_balance()
            .unwrap()
            .collect_vec();

        // then
        assert_groups_identical(
            &contract_balance_groups,
            decoded_contract_balance,
            skip_n_groups,
        );
    }

    #[test]
    fn roundtrip_json_coins() {
        // given
        let skip_n_groups = 3;
        let group_size = 100;
        let temp_dir = tempfile::tempdir().unwrap();
        let file = temp_dir.path().join("state_config.json");

        let mut encoder = Encoder::json(&file);
        let mut group_generator =
            GroupGenerator::new(StdRng::seed_from_u64(0), group_size, 10);

        // when
        let coin_groups =
            group_generator.for_each_group(|group| encoder.write_coins(group));
        encoder.close().unwrap();

        let decoded_coins = Decoder::json(file, group_size)
            .unwrap()
            .coins()
            .unwrap()
            .collect_vec();

        // then
        assert_groups_identical(&coin_groups, decoded_coins, skip_n_groups);
    }

    #[test]
    fn roundtrip_json_messages() {
        // given
        let skip_n_groups = 3;
        let group_size = 100;
        let temp_dir = tempfile::tempdir().unwrap();
        let file = temp_dir.path().join("state_config.json");

        let mut encoder = Encoder::json(&file);
        let mut group_generator =
            GroupGenerator::new(StdRng::seed_from_u64(0), group_size, 10);

        // when
        let message_groups =
            group_generator.for_each_group(|group| encoder.write_messages(group));
        encoder.close().unwrap();

        let decoded_messages = Decoder::json(&file, group_size)
            .unwrap()
            .messages()
            .unwrap()
            .collect_vec();

        // then
        assert_groups_identical(&message_groups, decoded_messages, skip_n_groups);
    }

    #[test]
    fn roundtrip_json_contracts() {
        // given
        let skip_n_groups = 3;
        let group_size = 100;
        let temp_dir = tempfile::tempdir().unwrap();
        let file = temp_dir.path().join("state_config.json");

        let mut encoder = Encoder::json(&file);
        let mut group_generator =
            GroupGenerator::new(StdRng::seed_from_u64(0), group_size, 10);

        // when
        let contract_groups =
            group_generator.for_each_group(|group| encoder.write_contracts(group));
        encoder.close().unwrap();

        let decoded_contracts = Decoder::json(&file, group_size)
            .unwrap()
            .contracts()
            .unwrap()
            .collect_vec();

        // then
        assert_groups_identical(&contract_groups, decoded_contracts, skip_n_groups);
    }

    #[test]
    fn roundtrip_json_contract_state() {
        // given
        let skip_n_groups = 3;
        let group_size = 100;
        let temp_dir = tempfile::tempdir().unwrap();
        let file = temp_dir.path().join("state_config.json");

        let mut encoder = Encoder::json(&file);
        let mut group_generator =
            GroupGenerator::new(StdRng::seed_from_u64(0), group_size, 10);

        // when
        let contract_state_groups =
            group_generator.for_each_group(|group| encoder.write_contract_state(group));
        encoder.close().unwrap();

        let decoded_contract_state = Decoder::json(&file, group_size)
            .unwrap()
            .contract_state()
            .unwrap()
            .collect_vec();

        // then
        assert_groups_identical(
            &contract_state_groups,
            decoded_contract_state,
            skip_n_groups,
        );
    }

    #[test]
    fn roundtrip_json_contract_balance() {
        // given
        let skip_n_groups = 3;
        let group_size = 100;
        let temp_dir = tempfile::tempdir().unwrap();
        let file = temp_dir.path().join("state_config.json");

        let mut encoder = Encoder::json(&file);
        let mut group_generator =
            GroupGenerator::new(StdRng::seed_from_u64(0), group_size, 10);

        // when
        let contract_balance_groups =
            group_generator.for_each_group(|group| encoder.write_contract_balance(group));
        encoder.close().unwrap();

        let decoded_contract_balance = Decoder::json(&file, group_size)
            .unwrap()
            .contract_balance()
            .unwrap()
            .collect_vec();

        // then
        assert_groups_identical(
            &contract_balance_groups,
            decoded_contract_balance,
            skip_n_groups,
        );
    }

    struct GroupGenerator<R> {
        rand: R,
        group_size: usize,
        num_groups: usize,
    }

    impl<R: ::rand::RngCore> GroupGenerator<R> {
        fn new(rand: R, group_size: usize, num_groups: usize) -> Self {
            Self {
                rand,
                group_size,
                num_groups,
            }
        }
        fn for_each_group<T: Randomize + Clone>(
            &mut self,
            mut f: impl FnMut(Vec<T>) -> anyhow::Result<()>,
        ) -> Vec<Group<T>> {
            let groups = self.generate_groups();
            for group in &groups {
                f(group.data.clone()).unwrap();
            }
            groups
        }
        fn generate_groups<T: Randomize>(&mut self) -> Vec<Group<T>> {
            ::std::iter::repeat_with(|| T::randomize(&mut self.rand))
                .chunks(self.group_size)
                .into_iter()
                .map(|chunk| chunk.collect_vec())
                .enumerate()
                .map(|(index, data)| Group { index, data })
                .take(self.num_groups)
                .collect()
        }
    }

    fn assert_groups_identical<T>(
        original: &[Group<T>],
        read: impl IntoIterator<Item = Result<Group<T>, anyhow::Error>>,
        skip: usize,
    ) where
        Vec<T>: PartialEq,
        T: PartialEq + std::fmt::Debug,
    {
        pretty_assertions::assert_eq!(
            original[skip..],
            read.into_iter()
                .skip(skip)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        );
    }
}
