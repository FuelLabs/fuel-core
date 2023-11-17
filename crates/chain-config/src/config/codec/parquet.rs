mod read;
mod schema;
mod write;

pub use read::*;
pub use schema::*;
pub use write::*;

#[cfg(test)]
mod tests {
    use itertools::Itertools;

    use crate::CoinConfig;

    use std::iter::repeat_with;

    use crate::config::codec::BatchWriterTrait;
    use bytes::Bytes;

    use crate::config::codec::Batch;

    // #[cfg(feature = "random")]
    // #[test]
    // fn encodes_and_decodes_coins() {
    // given
    // use crate::config::codec::parquet::{
    // read::ParquetBatchReader,
    // write::ParquetBatchWriter,
    // };
    // let coins = repeat_with(|| CoinConfig::random(&mut rand::thread_rng()))
    // .take(100)
    // .collect_vec();
    //
    // let mut writer = ParquetBatchWriter::<_, CoinConfig>::new(
    // vec![],
    // parquet::basic::Compression::UNCOMPRESSED,
    // )
    // .unwrap();
    //
    // when
    // writer.write_batch(coins.clone()).unwrap();
    //
    // then
    // let reader =
    // ParquetBatchReader::<_, CoinConfig>::new(Bytes::from(writer.into_inner()))
    // .unwrap();
    //
    // let decoded_codes = reader
    // .batches()
    // .into_iter()
    // .collect::<Result<Vec<_>, _>>()
    // .unwrap();
    //
    // assert_eq!(
    // vec![Batch {
    // data: coins,
    // group_index: 0
    // }],
    // decoded_codes
    // );
    // }
}
