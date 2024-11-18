use crate::ports::P2pDb;
use fuel_core_metrics::p2p_metrics::{
    increment_p2p_req_res_cache_hits,
    increment_p2p_req_res_cache_misses,
};
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::SealedBlockHeader,
    services::p2p::Transactions,
};
use quick_cache::sync::Cache;
use std::{
    ops::Range,
    sync::Arc,
};

type BlockHeight = u32;

pub(super) struct CachedView {
    sealed_block_headers: Cache<BlockHeight, SealedBlockHeader>,
    transactions_on_blocks: Cache<BlockHeight, Transactions>,
    metrics: bool,
}

impl CachedView {
    pub fn new(capacity: usize, metrics: bool) -> Self {
        Self {
            sealed_block_headers: Cache::new(capacity),
            transactions_on_blocks: Cache::new(capacity),
            metrics,
        }
    }

    fn update_metrics<U>(&self, update_fn: U)
    where
        U: FnOnce(),
    {
        if self.metrics {
            update_fn()
        }
    }

    fn get_from_cache_or_db<V, T, F>(
        &self,
        cache: &Cache<u32, T>,
        view: &V,
        range: Range<u32>,
        fetch_fn: F,
    ) -> StorageResult<Option<Vec<Arc<T>>>>
    where
        V: P2pDb,
        T: Clone,
        F: Fn(&V, Range<u32>) -> StorageResult<Option<Vec<T>>>,
    {
        let mut items = Vec::new();
        let mut missing_start = None;

        for height in range.clone() {
            if let Some(item) = cache.get(&height) {
                items.push(item.into());
            } else {
                missing_start = Some(height);
                break;
            }
        }

        let Some(missing_start) = missing_start else {
            self.update_metrics(increment_p2p_req_res_cache_hits);
            return Ok(Some(items));
        };

        let missing_range = missing_start..range.end;

        self.update_metrics(increment_p2p_req_res_cache_misses);
        if let Some(fetched_items) = fetch_fn(view, missing_range.clone())? {
            for (height, item) in missing_range.zip(fetched_items.iter()) {
                cache.insert(height, item.clone());
                items.push(item.clone().into());
            }

            return Ok(Some(items));
        }

        Ok(None)
    }

    pub(crate) fn get_sealed_headers<V>(
        &self,
        view: &V,
        block_height_range: Range<u32>,
    ) -> StorageResult<Option<Vec<Arc<SealedBlockHeader>>>>
    where
        V: P2pDb,
    {
        self.get_from_cache_or_db(
            &self.sealed_block_headers,
            view,
            block_height_range,
            V::get_sealed_headers,
        )
    }

    pub(crate) fn get_transactions<V>(
        &self,
        view: &V,
        block_height_range: Range<u32>,
    ) -> StorageResult<Option<Vec<Arc<Transactions>>>>
    where
        V: P2pDb,
    {
        self.get_from_cache_or_db(
            &self.transactions_on_blocks,
            view,
            block_height_range,
            V::get_transactions,
        )
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
        let cached_view = CachedView::new(10, false);

        let block_height_range = 0..100;
        let sealed_headers = default_sealed_headers(block_height_range.clone());
        let sealed_headers_heap = sealed_headers.iter().cloned().map(Arc::new).collect();
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
        assert_eq!(result, Some(sealed_headers_heap));
    }

    #[tokio::test]
    async fn cached_view__get_sealed_headers__cache_miss() {
        // given
        let sender = Arc::new(Notify::new());
        let db = FakeDb {
            sender: sender.clone(),
            values: true,
        };
        let cached_view = CachedView::new(10, false);

        // when
        let notified = sender.notified();
        let block_height_range = 0..100;
        let sealed_headers = default_sealed_headers(block_height_range.clone());
        let sealed_headers_heap = sealed_headers.iter().cloned().map(Arc::new).collect();
        let result = cached_view
            .get_sealed_headers(&db, block_height_range.clone())
            .unwrap();

        // then
        notified.await;
        assert_eq!(result, Some(sealed_headers_heap));
    }

    #[tokio::test]
    async fn cached_view__when_response_is_none__get_sealed_headers__cache_miss() {
        // given
        let sender = Arc::new(Notify::new());
        let db = FakeDb {
            sender: sender.clone(),
            values: false,
        };
        let cached_view = CachedView::new(10, false);

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
        let cached_view = CachedView::new(10, false);

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
        let cached_view = CachedView::new(10, false);

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
        let cached_view = CachedView::new(10, false);

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

    #[tokio::test]
    async fn cached_view__when_lru_is_full_it_makes_call_to_db() {
        // given
        let cache_capacity = 10;
        let sender = Arc::new(Notify::new());
        let db = FakeDb {
            sender: sender.clone(),
            values: true,
        };
        let cached_view = CachedView::new(cache_capacity, false);

        // when
        let block_height_range = 0..u32::try_from(cache_capacity).unwrap() + 1;
        let _ = cached_view
            .get_transactions(&db, block_height_range.clone())
            .unwrap();

        let notified = sender.notified();
        let result = cached_view
            .get_transactions(&db, block_height_range.clone())
            .unwrap();

        // then
        notified.await;
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn cached_view__when_lru_is_partially_full_it_does_not_make_call_to_db() {
        // given
        let cache_capacity = 100;
        let sender = Arc::new(Notify::new());
        let db = FakeDb {
            sender: sender.clone(),
            values: true,
        };
        let cached_view = CachedView::new(cache_capacity, false);

        // when
        let block_height_range = 0..10;
        let _ = cached_view
            .get_transactions(&db, block_height_range.clone())
            .unwrap();

        let notified = sender.notified();
        let _ = cached_view
            .get_transactions(&db, block_height_range.clone())
            .unwrap();

        // then
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), notified)
                .await
                .is_err()
        )
    }
}
