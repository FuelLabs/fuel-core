#![allow(unused)]

use std::{
    collections::BTreeMap,
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
        max_chunk_size: usize,
    ) -> futures::stream::Iter<std::vec::IntoIter<CachedDataBatch>> {
        let end = (*range.end()).saturating_add(1);
        let chunk_size_u32 = u32::try_from(max_chunk_size)
            .expect("The size of the chunk can't exceed `u32`");
        let cache_iter: Vec<(u32, CachedData)> = {
            let lock = self.0.lock();
            lock.range(range.clone())
                .map(|(k, v)| (*k, v.clone()))
                .collect()
        };
        let mut current_height = *range.start();
        // Build a stream of futures that will be splitted in chunks of available data and missing data to fetch with a maximum size of `max_chunk_size` for each chunk.
        let mut chunks = Vec::new();
        let mut current_chunk = CachedDataBatch::None(0..0);
        for (height, data) in cache_iter {
            if height == current_height {
                current_chunk = match (current_chunk, data) {
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
                        batch.range = batch.range.start..height.saturating_add(1);
                        batch.results.push(data);
                        CachedDataBatch::Headers(batch)
                    }
                    (CachedDataBatch::Blocks(mut batch), CachedData::Block(data)) => {
                        batch.range = batch.range.start..height.saturating_add(1);
                        batch.results.push(data);
                        CachedDataBatch::Blocks(batch)
                    }
                    (
                        CachedDataBatch::Headers(headers_batch),
                        CachedData::Block(block),
                    ) => {
                        chunks.push(CachedDataBatch::Headers(headers_batch));
                        CachedDataBatch::Blocks(Batch::new(
                            None,
                            height..height.saturating_add(1),
                            vec![block],
                        ))
                    }
                    (
                        CachedDataBatch::Blocks(blocks_batch),
                        CachedData::Header(header),
                    ) => {
                        chunks.push(CachedDataBatch::Blocks(blocks_batch));
                        CachedDataBatch::Headers(Batch::new(
                            None,
                            height..height.saturating_add(1),
                            vec![header],
                        ))
                    }
                };
                // Check the chunk limit and push the current chunk if it is full.
                current_chunk = match current_chunk {
                    CachedDataBatch::Headers(batch) => {
                        if batch.results.len() >= max_chunk_size {
                            chunks.push(CachedDataBatch::Headers(batch));
                            CachedDataBatch::None(current_height..current_height)
                        } else {
                            CachedDataBatch::Headers(batch)
                        }
                    }
                    CachedDataBatch::Blocks(batch) => {
                        if batch.results.len() >= max_chunk_size {
                            chunks.push(CachedDataBatch::Blocks(batch));
                            CachedDataBatch::None(current_height..current_height)
                        } else {
                            CachedDataBatch::Blocks(batch)
                        }
                    }
                    CachedDataBatch::None(range) => CachedDataBatch::None(range),
                }
            } else {
                // Push the current chunk of cached if it is not empty.
                match current_chunk {
                    CachedDataBatch::Headers(batch) => {
                        chunks.push(CachedDataBatch::Headers(batch));
                    }
                    CachedDataBatch::Blocks(batch) => {
                        chunks.push(CachedDataBatch::Blocks(batch));
                    }
                    CachedDataBatch::None(_) => {}
                }

                // Push the missing chunks.
                let missing_chunks = (current_height..height)
                    .step_by(max_chunk_size)
                    .map(move |chunk_start| {
                        let block_end =
                            chunk_start.saturating_add(chunk_size_u32).min(end);
                        CachedDataBatch::None(chunk_start..block_end)
                    });
                chunks.extend(missing_chunks);

                // Prepare the next chunk.
                current_chunk = match data {
                    CachedData::Header(data) => CachedDataBatch::Headers(Batch::new(
                        None,
                        height..height.saturating_add(1),
                        vec![data],
                    )),
                    CachedData::Block(data) => CachedDataBatch::Blocks(Batch::new(
                        None,
                        height..height.saturating_add(1),
                        vec![data],
                    )),
                };
            }
            current_height = height.saturating_add(1);
        }
        // Push the last chunk of cached if it is not empty.
        match current_chunk {
            CachedDataBatch::Headers(batch) => {
                chunks.push(CachedDataBatch::Headers(batch));
            }
            CachedDataBatch::Blocks(batch) => {
                chunks.push(CachedDataBatch::Blocks(batch));
            }
            CachedDataBatch::None(_) => {}
        }
        // Push the last missing chunk.
        if current_height <= *range.end() {
            // Include the last height.
            let missing_chunks =
                (current_height..end)
                    .step_by(max_chunk_size)
                    .map(move |chunk_start| {
                        let block_end =
                            chunk_start.saturating_add(chunk_size_u32).min(end);
                        CachedDataBatch::None(chunk_start..block_end)
                    });
            chunks.extend(missing_chunks);
        }
        futures::stream::iter(chunks)
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
    use std::ops::{
        Range,
        RangeInclusive,
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
            .get_chunks(asked_range, max_chunk_size)
            .collect()
            .await
    }
}
