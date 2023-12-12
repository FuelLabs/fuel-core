mod decoder;
mod encoder;
mod parquet;

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

    use itertools::Itertools;
    use rand::{
        rngs::StdRng,
        SeedableRng,
    };

    use crate::Randomize;

    use super::*;

    #[test]
    fn roundtrip_parquet() {
        let skip_n_groups = 3;

        let temp_dir = tempfile::tempdir().unwrap();
        let mut encoder = Encoder::parquet(temp_dir.path(), 1).unwrap();

        let rng = StdRng::seed_from_u64(0);
        let mut group_generator = GroupGenerator::new(rng, 100, 10);

        let coin_groups =
            group_generator.for_each_group(|group| encoder.write_coins(group));
        let message_groups =
            group_generator.for_each_group(|group| encoder.write_messages(group));
        let contract_groups =
            group_generator.for_each_group(|group| encoder.write_contracts(group));
        let contract_state_groups =
            group_generator.for_each_group(|group| encoder.write_contract_state(group));
        let contract_balance_groups =
            group_generator.for_each_group(|group| encoder.write_contract_balance(group));

        encoder.close().unwrap();

        let state_reader = Decoder::parquet(temp_dir.path());
        assert_groups_identical(
            &coin_groups,
            state_reader.coins().unwrap(),
            skip_n_groups,
        );
        assert_groups_identical(
            &message_groups,
            state_reader.messages().unwrap(),
            skip_n_groups,
        );
        assert_groups_identical(
            &contract_groups,
            state_reader.contracts().unwrap(),
            skip_n_groups,
        );
        assert_groups_identical(
            &contract_state_groups,
            state_reader.contract_state().unwrap(),
            skip_n_groups,
        );
        assert_groups_identical(
            &contract_balance_groups,
            state_reader.contract_balance().unwrap(),
            skip_n_groups,
        );
    }

    #[test]
    fn roundtrip_json() {
        let skip_n_groups = 3;
        let group_size = 100;

        let temp_dir = tempfile::tempdir().unwrap();
        let mut encoder = Encoder::json(temp_dir.path());

        let rng = StdRng::seed_from_u64(0);
        let mut group_generator = GroupGenerator::new(rng, group_size, 10);

        let coin_groups =
            group_generator.for_each_group(|group| encoder.write_coins(group));

        let message_groups =
            group_generator.for_each_group(|group| encoder.write_messages(group));

        let contract_groups =
            group_generator.for_each_group(|group| encoder.write_contracts(group));

        let contract_state_groups =
            group_generator.for_each_group(|group| encoder.write_contract_state(group));

        let contract_balance_groups =
            group_generator.for_each_group(|group| encoder.write_contract_balance(group));

        encoder.close().unwrap();

        let state_reader = Decoder::json(temp_dir.path(), group_size).unwrap();

        assert_groups_identical(
            &coin_groups,
            state_reader.coins().unwrap(),
            skip_n_groups,
        );

        assert_groups_identical(
            &message_groups,
            state_reader.messages().unwrap(),
            skip_n_groups,
        );

        assert_groups_identical(
            &contract_groups,
            state_reader.contracts().unwrap(),
            skip_n_groups,
        );

        assert_groups_identical(
            &contract_state_groups,
            state_reader.contract_state().unwrap(),
            skip_n_groups,
        );

        assert_groups_identical(
            &contract_balance_groups,
            state_reader.contract_balance().unwrap(),
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
