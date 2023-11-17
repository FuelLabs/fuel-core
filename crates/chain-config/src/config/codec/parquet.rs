mod read;
mod schema;
mod write;

pub use read::*;

#[cfg(test)]
mod tests {
    use itertools::Itertools;

    use crate::{
        CoinConfig,
        ContractConfig,
        MessageConfig,
    };

    use std::iter::repeat_with;

    use crate::config::codec::BatchWriter;
    use bytes::Bytes;

    use crate::config::codec::Batch;

    use crate::config::codec::parquet::{
        read::ParquetBatchReader,
        write::ParquetBatchWriter,
    };

    #[cfg(feature = "random")]
    #[test]
    fn encodes_and_decodes_coins() {
        // given
        let coins = repeat_with(|| CoinConfig::random(&mut rand::thread_rng()))
            .take(100)
            .collect_vec();

        let mut writer = ParquetBatchWriter::<_, CoinConfig>::new(
            vec![],
            parquet::basic::Compression::UNCOMPRESSED,
        )
        .unwrap();

        // when
        writer.write_batch(coins.clone()).unwrap();

        // then
        let reader =
            ParquetBatchReader::<_, CoinConfig>::new(Bytes::from(writer.into_inner()))
                .unwrap();

        let decoded_codes = reader.into_iter().collect::<Result<Vec<_>, _>>().unwrap();

        assert_eq!(
            vec![Batch {
                data: coins,
                group_index: 0
            }],
            decoded_codes
        );
    }

    #[cfg(feature = "random")]
    #[test]
    fn encodes_and_decodes_messages() {
        // given
        let messages = repeat_with(|| MessageConfig::random(&mut rand::thread_rng()))
            .take(100)
            .collect_vec();

        let mut writer = ParquetBatchWriter::<_, MessageConfig>::new(
            vec![],
            parquet::basic::Compression::UNCOMPRESSED,
        )
        .unwrap();

        // when
        writer.write_batch(messages.clone()).unwrap();

        // then
        let reader =
            ParquetBatchReader::<_, MessageConfig>::new(Bytes::from(writer.into_inner()))
                .unwrap();

        let decoded_codes = reader.into_iter().collect::<Result<Vec<_>, _>>().unwrap();

        assert_eq!(
            vec![Batch {
                data: messages,
                group_index: 0
            }],
            decoded_codes
        );
    }

    #[cfg(feature = "random")]
    #[test]
    fn encodes_and_decodes_contracts() {
        // given
        let contracts = repeat_with(|| ContractConfig::random(&mut rand::thread_rng()))
            .take(100)
            .collect_vec();

        let mut writer = ParquetBatchWriter::<_, ContractConfig>::new(
            vec![],
            parquet::basic::Compression::UNCOMPRESSED,
        )
        .unwrap();

        // when
        writer.write_batch(contracts.clone()).unwrap();

        // then
        let reader = ParquetBatchReader::<_, ContractConfig>::new(Bytes::from(
            writer.into_inner(),
        ))
        .unwrap();

        let decoded_codes = reader.into_iter().collect::<Result<Vec<_>, _>>().unwrap();

        assert_eq!(
            vec![Batch {
                data: contracts,
                group_index: 0
            }],
            decoded_codes
        );
    }
}
