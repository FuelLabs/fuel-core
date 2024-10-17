#![allow(unused)]

use std::{collections::BTreeMap, ops::{Range, RangeInclusive}};

use fuel_core_services::SharedMutex;
use fuel_core_types::blockchain::{SealedBlock, SealedBlockHeader};

use super::Batch;

/// The cache that stores the fetched headers and blocks.
#[derive(Clone, Debug)]
pub struct Cache(SharedMutex<BTreeMap<u32, CachedData>>);

#[derive(Debug, Clone)]
pub enum CachedData {
    Header(SealedBlockHeader),
    Block(SealedBlock),
}

/// The data that is fetched either in the network or in the cache for a range of headers or blocks.
#[derive(Debug, Clone)]
enum BlockHeaderData {
    /// The headers (or full blocks) have been fetched and checked.
    Cached(CachedData),
    /// The headers has just been fetched from the network.
    Fetched(Batch<SealedBlockHeader>),
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

    pub fn get_chunks(&self, range: RangeInclusive<u32>, max_chunk_size: usize) -> futures::stream::Iter<std::vec::IntoIter<(Range<u32>, Option<Vec<CachedData>>)>> {
        let end = (*range.end()).saturating_add(1);
        let chunk_size_u32 =
                    u32::try_from(max_chunk_size).expect("The size of the chunk can't exceed `u32`");
        let lock = self.0.lock();
        let cache_iter = lock.range(range.clone());
        let mut start_current_chunk = *range.start();
        let mut current_height = *range.start();
        // Build a stream of futures that will be splitted in chunks of available data and missing data to fetch with a maximum size of `max_chunk_size` for each chunk.
        let mut chunks = Vec::new();
        let mut current_chunk = Vec::new();
        for (height, data) in cache_iter {
            if *height == current_height {
                current_chunk.push(data.clone());
                current_height = *height + 1;
                if current_chunk.len() >= max_chunk_size {
                    chunks.push((start_current_chunk..current_height, Some(current_chunk)));
                    start_current_chunk = current_height;
                    current_chunk = Vec::new();
                }
            } else {
                // Push the current chunk of cached if it is not empty.
                if !current_chunk.is_empty() {
                    chunks.push((start_current_chunk..current_height, Some(current_chunk)));
                }
                // Push the missing chunks.
                let missing_chunks = (current_height..*height).step_by(max_chunk_size).map(move |chunk_start| {
                    let block_end = chunk_start.saturating_add(chunk_size_u32).min(end);
                    (chunk_start..block_end, None)
                });
                chunks.extend(missing_chunks);

                // Prepare the next chunk.
                start_current_chunk = *height;
                current_chunk = Vec::new();
                current_chunk.push(data.clone());
                current_height = *height + 1;
            }
        }
        // Push the last chunk of cached if it is not empty.
        if !current_chunk.is_empty() {
            chunks.push((start_current_chunk..current_height, Some(current_chunk)));
        }
        // Push the last missing chunk.
        if current_height < *range.end() {
            // Include the last height.
            let missing_chunks = (current_height..end).step_by(max_chunk_size).map(move |chunk_start| {
                let block_end = chunk_start.saturating_add(chunk_size_u32).min(end);
                (chunk_start..block_end, None)
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
    use std::ops::Range;
    use fuel_core_types::blockchain::{consensus::Sealed, header::BlockHeader};
    use futures::StreamExt;
    use crate::import::{cache::{CachedData, Cache}, Batch};

    #[tokio::test]
    async fn test_one_element_and_empty_ranges() {
        let mut cache = Cache::new();
        let mut header = BlockHeader::default();
        header.set_block_height(0.into());
        let header = Sealed {
            entity: header,
            consensus: Default::default(),
        };
        cache.insert_headers(Batch::new(None, 0..1, vec![header.clone()]));
        let chunks = cache.get_chunks(0..=10, 5);
        let chunks: Vec<(Range<u32>, Option<Vec<CachedData>>)> = chunks.collect().await;
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].0, 0..1);
        assert_eq!(chunks[1].0, 1..6);
        assert_eq!(chunks[2].0, 6..11);
        assert_eq!(chunks[0].1.as_ref().unwrap().len(), 1);
        assert_eq!(chunks[1].1.is_none(), true);
        assert_eq!(chunks[2].1.is_none(), true);
    }

    #[tokio::test]
    async fn test_only_full_ranges() {
        let mut cache = Cache::new();
        let mut headers = Vec::new();
        for i in 0..11 {
            let mut header = BlockHeader::default();
            header.set_block_height(i.into());
            let header = Sealed {
                entity: header.clone(),
                consensus: Default::default(),
            };
            headers.push(header);
        }
        cache.insert_headers(Batch::new(None, 0..11, headers));
        let chunks = cache.get_chunks(0..=10, 3);
        let chunks: Vec<(Range<u32>, Option<Vec<CachedData>>)> = chunks.collect().await;
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0].0, 0..3);
        assert_eq!(chunks[1].0, 3..6);
        assert_eq!(chunks[2].0, 6..9);
        assert_eq!(chunks[3].0, 9..11);
        assert_eq!(chunks.iter().all(|(_, b)| b.is_some()), true);
        assert_eq!(chunks[0].1.as_ref().unwrap().len(), 3);
        assert_eq!(chunks[1].1.as_ref().unwrap().len(), 3);
        assert_eq!(chunks[2].1.as_ref().unwrap().len(), 3);
        assert_eq!(chunks[3].1.as_ref().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_only_empty_ranges() {
        let mut cache = Cache::new();
        let chunks = cache.get_chunks(0..=10, 3);
        let chunks: Vec<(Range<u32>, Option<Vec<CachedData>>)> = chunks.collect().await;
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0].0, 0..3);
        assert_eq!(chunks[1].0, 3..6);
        assert_eq!(chunks[2].0, 6..9);
        assert_eq!(chunks[3].0, 9..11);
        assert_eq!(chunks.iter().all(|(_, b)| b.is_none()), true);
    }

    #[tokio::test]
    async fn test_ranges_cached_and_empty() {
        let mut cache = Cache::new();
        let mut headers = Vec::new();
        for i in 0..6 {
            let mut header = BlockHeader::default();
            header.set_block_height(i.into());
            let header = Sealed {
                entity: header.clone(),
                consensus: Default::default(),
            };
            headers.push(header);
        }
        cache.insert_headers(Batch::new(None, 0..6, headers));
        let chunks = cache.get_chunks(0..=10, 3);
        let chunks: Vec<(Range<u32>, Option<Vec<CachedData>>)> = chunks.collect().await;
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0].0, 0..3);
        assert_eq!(chunks[1].0, 3..6);
        assert_eq!(chunks[2].0, 6..9);
        assert_eq!(chunks[3].0, 9..11);
        assert_eq!(chunks[0].1.as_ref().unwrap().len(), 3);
        assert_eq!(chunks[1].1.as_ref().unwrap().len(), 3);
        assert_eq!(chunks[2].1.is_none(), true);
        assert_eq!(chunks[3].1.is_none(), true);
    }
}