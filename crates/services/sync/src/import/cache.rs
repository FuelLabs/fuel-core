#![allow(unused)]

use std::{
    collections::BTreeMap,
    num::NonZeroUsize,
    ops::{
        Range,
        RangeInclusive,
    },
};

use fuel_core_services::SharedMutex;
use fuel_core_types::blockchain::{
    SealedBlock,
    SealedBlockHeader,
};

use itertools::Itertools;

use super::Batch;

/// The cache that stores the fetched headers and blocks.
#[derive(Clone, Debug)]
pub struct Cache(SharedMutex<BTreeMap<u32, CachedData>>);

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum CachedData {
    Header(SealedBlockHeader),
    Block(SealedBlock),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CachedDataBatch {
    Headers(Batch<SealedBlockHeader>),
    Blocks(Batch<SealedBlock>),
    None(Range<u32>),
}

impl CachedDataBatch {
    pub fn is_range_empty(&self) -> bool {
        match self {
            CachedDataBatch::None(range) => range.is_empty(),
            CachedDataBatch::Blocks(batch) => batch.range.is_empty(),
            CachedDataBatch::Headers(batch) => batch.range.is_empty(),
        }
    }
}

impl Cache {
    pub fn new() -> Self {
        Self(SharedMutex::new(BTreeMap::new()))
    }

    pub fn insert_blocks(&mut self, batch: Batch<SealedBlock>) {
        let mut lock = self.0.lock();
        for block in batch.results {
            lock.insert(**block.entity.header().height(), CachedData::Block(block));
        }
    }

    pub fn insert_headers(&mut self, batch: Batch<SealedBlockHeader>) {
        let mut lock = self.0.lock();
        for header in batch.results {
            lock.insert(**header.entity.height(), CachedData::Header(header));
        }
    }

    pub fn get_chunks(
        &self,
        range: RangeInclusive<u32>,
        max_chunk_size: NonZeroUsize,
    ) -> futures::stream::Iter<std::vec::IntoIter<CachedDataBatch>> {
        let end = (*range.end()).saturating_add(1);
        let chunk_size_u32 = u32::try_from(max_chunk_size.get())
            .expect("The size of the chunk can't exceed `u32`");

        let cache_iter = self.collect_cache_data(range.clone());
        let mut current_height = *range.start();
        let mut chunks = Vec::new();
        let mut current_chunk = CachedDataBatch::None(0..0);

        for (height, data) in cache_iter {
            // We have a range missing in our cache.
            // Push the existing chunk and push chunks of missing data
            if height != current_height {
                Self::push_current_chunk(&mut chunks, current_chunk, max_chunk_size);
                current_chunk = CachedDataBatch::None(0..0);
                Self::push_missing_chunks(
                    &mut chunks,
                    current_height,
                    height,
                    max_chunk_size,
                    chunk_size_u32,
                    end,
                );
                current_height = height;
            }
            // Either accumulate in the same chunk or transition if the data is not the same as the current chunk
            current_chunk = Self::handle_current_chunk(
                current_chunk,
                data,
                height,
                &mut chunks,
                max_chunk_size,
            );
            current_height = height.saturating_add(1);
        }

        Self::push_current_chunk(&mut chunks, current_chunk, max_chunk_size);
        if current_height <= end {
            Self::push_missing_chunks(
                &mut chunks,
                current_height,
                end,
                max_chunk_size,
                chunk_size_u32,
                end,
            );
        }
        futures::stream::iter(chunks)
    }

    // Split the chunk in chunks of `max_chunk_size` and push them in `chunks` vector.
    fn push_current_chunk(
        chunks: &mut Vec<CachedDataBatch>,
        chunk: CachedDataBatch,
        max_chunk_size: NonZeroUsize,
    ) {
        if chunk.is_range_empty() {
            return;
        }
        // Adds a vec of chunks that respect `max_chunk_size`
        match chunk {
            CachedDataBatch::Blocks(blocks) => chunks.extend(
                blocks
                    .range
                    .zip(blocks.results)
                    .chunks(max_chunk_size.into())
                    .into_iter()
                    .filter_map(|chunk| {
                        let (range, blocks): (Vec<u32>, Vec<SealedBlock>) = chunk.unzip();
                        let start_range = range.first()?;
                        let mut end_range = range.get(range.len().saturating_sub(1))?;
                        Some(CachedDataBatch::Blocks(Batch::new(
                            None,
                            *start_range..end_range.saturating_add(1),
                            blocks,
                        )))
                    }),
            ),
            CachedDataBatch::Headers(headers) => chunks.extend(
                headers
                    .range
                    .zip(headers.results)
                    .chunks(max_chunk_size.into())
                    .into_iter()
                    .filter_map(|chunk| {
                        let (range, headers): (Vec<u32>, Vec<SealedBlockHeader>) =
                            chunk.unzip();
                        let start_range = range.first()?;
                        let mut end_range = range.get(range.len().saturating_sub(1))?;
                        Some(CachedDataBatch::Headers(Batch::new(
                            None,
                            *start_range..end_range.saturating_add(1),
                            headers,
                        )))
                    }),
            ),
            CachedDataBatch::None(range) => chunks.push(CachedDataBatch::None(range)),
        }
    }

    fn collect_cache_data(&self, range: RangeInclusive<u32>) -> Vec<(u32, CachedData)> {
        let lock = self.0.lock();
        lock.range(range).map(|(k, v)| (*k, v.clone())).collect()
    }

    fn handle_current_chunk(
        current_chunk: CachedDataBatch,
        data: CachedData,
        height: u32,
        chunks: &mut Vec<CachedDataBatch>,
        max_chunk_size: NonZeroUsize,
    ) -> CachedDataBatch {
        match (current_chunk, data) {
            (CachedDataBatch::None(_), CachedData::Header(data)) => {
                CachedDataBatch::Headers(Batch::new(
                    None,
                    height..height.saturating_add(1),
                    vec![data],
                ))
            }
            (CachedDataBatch::None(_), CachedData::Block(data)) => {
                CachedDataBatch::Blocks(Batch::new(
                    None,
                    height..height.saturating_add(1),
                    vec![data],
                ))
            }
            (CachedDataBatch::Headers(mut batch), CachedData::Header(data)) => {
                tracing::warn!("Header data range in cache is not continuous.");
                debug_assert_eq!(batch.range.end, height);
                batch.range = batch.range.start..batch.range.end.saturating_add(1);
                batch.results.push(data);
                CachedDataBatch::Headers(batch)
            }
            (CachedDataBatch::Blocks(mut batch), CachedData::Block(data)) => {
                tracing::warn!("Block data range in cache is not continuous.");
                debug_assert_eq!(batch.range.end, height);
                batch.range = batch.range.start..batch.range.end.saturating_add(1);
                batch.results.push(data);
                CachedDataBatch::Blocks(batch)
            }
            (CachedDataBatch::Headers(headers_batch), CachedData::Block(block)) => {
                debug_assert_eq!(headers_batch.range.end, height);
                Self::push_current_chunk(
                    chunks,
                    CachedDataBatch::Headers(headers_batch),
                    max_chunk_size,
                );
                CachedDataBatch::Blocks(Batch::new(
                    None,
                    height..height.saturating_add(1),
                    vec![block],
                ))
            }
            (CachedDataBatch::Blocks(blocks_batch), CachedData::Header(header)) => {
                debug_assert_eq!(blocks_batch.range.end, height);
                Self::push_current_chunk(
                    chunks,
                    CachedDataBatch::Blocks(blocks_batch),
                    max_chunk_size,
                );
                CachedDataBatch::Headers(Batch::new(
                    None,
                    height..height.saturating_add(1),
                    vec![header],
                ))
            }
        }
    }

    fn push_missing_chunks(
        chunks: &mut Vec<CachedDataBatch>,
        current_height: u32,
        height: u32,
        max_chunk_size: NonZeroUsize,
        chunk_size_u32: u32,
        end: u32,
    ) {
        let missing_chunks = (current_height..height).step_by(max_chunk_size.into()).map(
            move |chunk_start| {
                let block_end = chunk_start.saturating_add(chunk_size_u32).min(end);
                CachedDataBatch::None(chunk_start..block_end)
            },
        );
        chunks.extend(missing_chunks);
    }

    pub fn remove_element(&mut self, height: &u32) {
        let mut lock = self.0.lock();
        lock.remove(height);
    }
}

#[cfg(test)]
mod tests {
    use crate::import::{
        cache::{
            Cache,
            CachedData,
            CachedDataBatch,
        },
        Batch,
    };
    use fuel_core_types::{
        blockchain::{
            block::Block,
            consensus::Sealed,
            header::BlockHeader,
        },
        fuel_tx::Bytes32,
        tai64::Tai64,
    };
    use futures::StreamExt;
    use std::{
        num::NonZeroUsize,
        ops::{
            Range,
            RangeInclusive,
        },
    };
    use test_case::test_case;

    fn create_header(height: u32) -> Sealed<BlockHeader> {
        Sealed {
            entity: BlockHeader::new_block(height.into(), Tai64::from_unix(0)),
            consensus: Default::default(),
        }
    }
    fn create_block(height: u32) -> Sealed<Block> {
        Sealed {
            entity: Block::new(
                (&create_header(height).entity).into(),
                Vec::new(),
                &[],
                Bytes32::default(),
            )
            .unwrap(),
            consensus: Default::default(),
        }
    }

    #[test_case(&[], &[], 3, 0..=10 => vec![
        CachedDataBatch::None(0..3),
        CachedDataBatch::None(3..6),
        CachedDataBatch::None(6..9),
        CachedDataBatch::None(9..11),
    ] ; "multiple empty chunks")]
    #[test_case(&[
        create_header(0)
    ], &[], 3, 0..=10 => vec![
        CachedDataBatch::Headers(Batch::new(None, 0..1, vec![create_header(0)])),
        CachedDataBatch::None(1..4),
        CachedDataBatch::None(4..7),
        CachedDataBatch::None(7..10),
        CachedDataBatch::None(10..11),
    ]; "one header and empty ranges")]
    #[test_case(&[
        create_header(0),
        create_header(1),
        create_header(2)
    ], &[], 3, 0..=10 => vec![
        CachedDataBatch::Headers(Batch::new(None, 0..3, vec![
            create_header(0),
            create_header(1),
            create_header(2)
        ])),
        CachedDataBatch::None(3..6),
        CachedDataBatch::None(6..9),
        CachedDataBatch::None(9..11),
    ]; "multiple headers and empty ranges")]
    #[test_case(&[], &[
        create_block(0)
    ], 3, 0..=10 => vec![
        CachedDataBatch::Blocks(Batch::new(None, 0..1, vec![create_block(0)])),
        CachedDataBatch::None(1..4),
        CachedDataBatch::None(4..7),
        CachedDataBatch::None(7..10),
        CachedDataBatch::None(10..11),
    ]; "one block and empty ranges")]
    #[test_case(&[
        create_header(0)
    ], &[
        create_block(1)
    ], 3, 0..=10 => vec![
        CachedDataBatch::Headers(Batch::new(None, 0..1, vec![create_header(0)])),
        CachedDataBatch::Blocks(Batch::new(None, 1..2, vec![create_block(1)])),
        CachedDataBatch::None(2..5),
        CachedDataBatch::None(5..8),
        CachedDataBatch::None(8..11),
    ]; "one header, one block and empty ranges")]
    #[test_case(&[
        create_header(0),
        create_header(1)
    ], &[
        create_block(2),
        create_block(3)
    ], 2, 0..=10 => vec![
        CachedDataBatch::Headers(Batch::new(None, 0..2, vec![
            create_header(0),
            create_header(1)
        ])),
        CachedDataBatch::Blocks(Batch::new(None, 2..4, vec![
            create_block(2),
            create_block(3)
        ])),
        CachedDataBatch::None(4..6),
        CachedDataBatch::None(6..8),
        CachedDataBatch::None(8..10),
        CachedDataBatch::None(10..11),
    ]; "multiple headers, multiple blocks and empty ranges")]
    #[test_case(&[
        create_header(0),
        create_header(1),
        create_header(2),
        create_header(3)
    ], &[
        create_block(4),
        create_block(5),
        create_block(6),
        create_block(7)
    ], 2, 0..=10 => vec![
        CachedDataBatch::Headers(Batch::new(None, 0..2, vec![
            create_header(0),
            create_header(1)
        ])),
        CachedDataBatch::Headers(Batch::new(None, 2..4, vec![
            create_header(2),
            create_header(3)
        ])),
        CachedDataBatch::Blocks(Batch::new(None, 4..6, vec![
            create_block(4),
            create_block(5)
        ])),
        CachedDataBatch::Blocks(Batch::new(None, 6..8, vec![
            create_block(6),
            create_block(7)
        ])),
        CachedDataBatch::None(8..10),
        CachedDataBatch::None(10..11),
    ]; "multiple headers, multiple blocks and empty ranges with smaller chunk size")]
    #[test_case(&[
        create_header(0),
        create_header(1),
        create_header(2),
        create_header(3)
    ], &[
        create_block(4),
        create_block(5),
        create_block(6),
        create_block(7)
    ], 2, 0..=7 => vec![
        CachedDataBatch::Headers(Batch::new(None, 0..2, vec![
            create_header(0),
            create_header(1)
        ])),
        CachedDataBatch::Headers(Batch::new(None, 2..4, vec![
            create_header(2),
            create_header(3)
        ])),
        CachedDataBatch::Blocks(Batch::new(None, 4..6, vec![
            create_block(4),
            create_block(5)
        ])),
        CachedDataBatch::Blocks(Batch::new(None, 6..8, vec![
            create_block(6),
            create_block(7)
        ])),
    ]; "multiple headers, multiple blocks with no empty ranges")]
    #[test_case(&[
        create_header(0),
        create_header(1),
        create_header(2)
    ], &[
        create_block(3),
        create_block(4),
        create_block(5)
    ], 3, 0..=5 => vec![
        CachedDataBatch::Headers(Batch::new(None, 0..3, vec![
            create_header(0),
            create_header(1),
            create_header(2)
        ])),
        CachedDataBatch::Blocks(Batch::new(None, 3..6, vec![
            create_block(3),
            create_block(4),
            create_block(5)
        ])),
    ]; "multiple headers, multiple blocks with no empty ranges and larger chunk size")]
    #[test_case(&[
        create_header(0),
        create_header(1)
    ], &[
        create_block(2),
        create_block(3)
    ], 2, 0..=3 => vec![
        CachedDataBatch::Headers(Batch::new(None, 0..2, vec![
            create_header(0),
            create_header(1)
        ])),
        CachedDataBatch::Blocks(Batch::new(None, 2..4, vec![
            create_block(2),
            create_block(3)
        ])),
    ]; "multiple headers, multiple blocks with no empty ranges and exact chunk size")]
    #[test_case(&[
        create_header(0),
        create_header(1),
        create_header(2)
    ], &[
        create_block(3),
        create_block(4),
        create_block(5)
    ], 1, 0..=5 => vec![
        CachedDataBatch::Headers(Batch::new(None, 0..1, vec![
            create_header(0)
        ])),
        CachedDataBatch::Headers(Batch::new(None, 1..2, vec![
            create_header(1)
        ])),
        CachedDataBatch::Headers(Batch::new(None, 2..3, vec![
            create_header(2)
        ])),
        CachedDataBatch::Blocks(Batch::new(None, 3..4, vec![
            create_block(3)
        ])),
        CachedDataBatch::Blocks(Batch::new(None, 4..5, vec![
            create_block(4)
        ])),
        CachedDataBatch::Blocks(Batch::new(None, 5..6, vec![
            create_block(5)
        ])),
    ]; "multiple headers, multiple blocks with max chunk size of 1")]
    #[test_case(&[
        create_header(0)
    ], &[
        create_block(1)
    ], 1, 0..=1 => vec![
        CachedDataBatch::Headers(Batch::new(None, 0..1, vec![
            create_header(0)
        ])),
        CachedDataBatch::Blocks(Batch::new(None, 1..2, vec![
            create_block(1)
        ])),
    ]; "one header, one block with max chunk size of 1")]
    #[test_case(&[], &[
        create_block(5)
    ], 1, 4..=6 => vec![
        CachedDataBatch::None(4..5),
        CachedDataBatch::Blocks(Batch::new(None, 5..6, vec![
            create_block(5)
        ])),
        CachedDataBatch::None(6..7),
    ]; "one block in empty range sandwich with max chunk size of 1")]
    #[tokio::test]
    async fn test_get_batch_scenarios(
        headers: &[Sealed<BlockHeader>],
        blocks: &[Sealed<Block>],
        max_chunk_size: usize,
        asked_range: RangeInclusive<u32>,
    ) -> Vec<CachedDataBatch> {
        let mut cache = Cache::new();
        cache.insert_headers(Batch::new(
            None,
            0..headers.len().try_into().unwrap(),
            headers.to_vec(),
        ));
        cache.insert_blocks(Batch::new(
            None,
            0..blocks.len().try_into().unwrap(),
            blocks.to_vec(),
        ));
        cache
            .get_chunks(asked_range, NonZeroUsize::try_from(max_chunk_size).unwrap())
            .collect()
            .await
    }
}
