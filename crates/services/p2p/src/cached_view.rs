use crate::ports::P2pDb;
use dashmap::DashMap;
use fuel_core_metrics::p2p_metrics::{
    increment_p2p_req_res_cache_hits,
    increment_p2p_req_res_cache_misses,
};
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::SealedBlockHeader,
    services::p2p::Transactions,
};
use std::ops::Range;

pub struct CachedView {
    // map block height to sealed block header
    sealed_block_headers: DashMap<u32, SealedBlockHeader>,
    // map block height to transactions
    transactions_on_blocks: DashMap<u32, Transactions>,
    metrics: bool,
}

impl CachedView {
    pub fn new(metrics: bool) -> Self {
        Self {
            sealed_block_headers: DashMap::new(),
            transactions_on_blocks: DashMap::new(),
            metrics,
        }
    }

    pub fn clear(&self) {
        self.sealed_block_headers.clear();
        self.transactions_on_blocks.clear();
    }

    fn update_metrics<U>(&self, update_fn: U)
    where
        U: FnOnce(),
    {
        if self.metrics {
            update_fn()
        }
    }

    pub(crate) fn get_sealed_headers<V>(
        &self,
        view: &V,
        block_height_range: Range<u32>,
    ) -> StorageResult<Option<Vec<SealedBlockHeader>>>
    where
        V: P2pDb,
    {
        let mut headers = Vec::new();
        let mut missing_ranges = block_height_range.clone();
        for block_height in block_height_range.clone() {
            if let Some(header) = self.sealed_block_headers.get(&block_height) {
                headers.push(header.clone());
            } else {
                // for the first block not in the cache, start a new range
                missing_ranges.start = block_height;
                break;
            }
        }

        if missing_ranges.is_empty() {
            self.update_metrics(increment_p2p_req_res_cache_hits);
            return Ok(Some(headers))
        }

        self.update_metrics(increment_p2p_req_res_cache_misses);
        let missing_headers = view.get_sealed_headers(missing_ranges.clone())?;
        if let Some(missing_headers) = &missing_headers {
            for header in missing_headers.iter() {
                self.sealed_block_headers
                    .insert((*header.entity.height()).into(), header.clone());
                headers.push(header.clone());
            }
        }
        Ok(missing_headers)
    }

    pub(crate) fn get_transactions<V>(
        &self,
        view: &V,
        block_height_range: Range<u32>,
    ) -> StorageResult<Option<Vec<Transactions>>>
    where
        V: P2pDb,
    {
        let mut transactions = Vec::new();
        let mut missing_ranges = block_height_range.clone();
        for block_height in block_height_range.clone() {
            if let Some(cached_tx) = self.transactions_on_blocks.get(&block_height) {
                transactions.push(cached_tx.clone());
            } else {
                // for the first block not in the cache, start a new range
                missing_ranges.start = block_height;
                break;
            }
        }

        if missing_ranges.is_empty() {
            self.update_metrics(increment_p2p_req_res_cache_hits);
            return Ok(Some(transactions))
        }

        self.update_metrics(increment_p2p_req_res_cache_misses);
        let transactions = view.get_transactions(missing_ranges.clone())?;
        if let Some(transactions) = &transactions {
            for (block_height, transactions_per_block) in
                missing_ranges.zip(transactions.iter())
            {
                self.transactions_on_blocks
                    .insert(block_height, transactions_per_block.clone());
            }
        }
        Ok(transactions)
    }
}

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_types::blockchain::consensus::Genesis;
    use std::sync::Arc;
    use tokio::sync::Notify;

    struct FakeDb {
        sender: Arc<Notify>,
        values: bool,
    }

    #[inline]
    fn default_sealed_headers(range: Range<u32>) -> Vec<SealedBlockHeader> {
        vec![SealedBlockHeader::default(); range.len()]
    }

    #[inline]
    fn default_transactions(range: Range<u32>) -> Vec<Transactions> {
        vec![Transactions::default(); range.len()]
    }

    impl P2pDb for FakeDb {
        fn get_sealed_headers(
            &self,
            range: Range<u32>,
        ) -> StorageResult<Option<Vec<SealedBlockHeader>>> {
            self.sender.notify_waiters();
            if !self.values {
                return Ok(None);
            }
            let headers = default_sealed_headers(range);
            Ok(Some(headers))
        }

        fn get_transactions(
            &self,
            range: Range<u32>,
        ) -> StorageResult<Option<Vec<Transactions>>> {
            self.sender.notify_waiters();
            if !self.values {
                return Ok(None);
            }
            let transactions = default_transactions(range);
            Ok(Some(transactions))
        }

        fn get_genesis(&self) -> StorageResult<Genesis> {
            self.sender.notify_waiters();
            Ok(Genesis::default())
        }
    }

    #[tokio::test]
    async fn cached_view__get_sealed_headers__cache_hit() {
        let sender = Arc::new(Notify::new());
        let db = FakeDb {
            sender: sender.clone(),
            values: true,
        };
        let cached_view = CachedView::new(false);

        let block_height_range = 0..100;
        let sealed_headers = default_sealed_headers(block_height_range.clone());
        for (block_height, header) in
            block_height_range.clone().zip(sealed_headers.iter())
        {
            cached_view
                .sealed_block_headers
                .insert(block_height, header.clone());
        }

        let result = cached_view
            .get_sealed_headers(&db, block_height_range.clone())
            .unwrap();
        assert_eq!(result, Some(sealed_headers));
    }

    #[tokio::test]
    async fn cached_view__get_sealed_headers__cache_miss() {
        // given
        let sender = Arc::new(Notify::new());
        let db = FakeDb {
            sender: sender.clone(),
            values: true,
        };
        let cached_view = CachedView::new(false);

        // when
        let notified = sender.notified();
        let block_height_range = 0..100;
        let sealed_headers = default_sealed_headers(block_height_range.clone());
        let result = cached_view
            .get_sealed_headers(&db, block_height_range.clone())
            .unwrap();

        // then
        notified.await;
        assert_eq!(result, Some(sealed_headers));
    }

    #[tokio::test]
    async fn cached_view__when_response_is_none__get_sealed_headers__cache_miss() {
        // given
        let sender = Arc::new(Notify::new());
        let db = FakeDb {
            sender: sender.clone(),
            values: false,
        };
        let cached_view = CachedView::new(false);

        // when
        let notified = sender.notified();
        let block_height_range = 0..100;
        let result = cached_view
            .get_sealed_headers(&db, block_height_range.clone())
            .unwrap();

        // then
        notified.await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn cached_view__get_transactions__cache_hit() {
        let sender = Arc::new(Notify::new());
        let db = FakeDb {
            sender: sender.clone(),
            values: true,
        };
        let cached_view = CachedView::new(false);

        let block_height_range = 0..100;
        let transactions = default_transactions(block_height_range.clone());

        for (block_height, transactions) in
            block_height_range.clone().zip(transactions.iter())
        {
            cached_view
                .transactions_on_blocks
                .insert(block_height, transactions.clone());
        }

        let result = cached_view
            .get_transactions(&db, block_height_range.clone())
            .unwrap();

        for (expected, actual) in transactions.iter().zip(result.unwrap().iter()) {
            assert_eq!(expected.0, actual.0);
        }
    }

    #[tokio::test]
    async fn cached_view__get_transactions__cache_miss() {
        // given
        let sender = Arc::new(Notify::new());
        let db = FakeDb {
            sender: sender.clone(),
            values: true,
        };
        let cached_view = CachedView::new(false);

        // when
        let notified = sender.notified();
        let block_height_range = 0..100;
        let transactions = default_transactions(block_height_range.clone());
        let result = cached_view
            .get_transactions(&db, block_height_range.clone())
            .unwrap();

        // then
        notified.await;
        for (expected, actual) in transactions.iter().zip(result.unwrap().iter()) {
            assert_eq!(expected.0, actual.0);
        }
    }

    #[tokio::test]
    async fn cached_view__when_response_is_none__get_transactions__cache_miss() {
        // given
        let sender = Arc::new(Notify::new());
        let db = FakeDb {
            sender: sender.clone(),
            values: false,
        };
        let cached_view = CachedView::new(false);

        // when
        let notified = sender.notified();
        let block_height_range = 0..100;
        let result = cached_view
            .get_transactions(&db, block_height_range.clone())
            .unwrap();

        // then
        notified.await;
        assert!(result.is_none());
    }
}
