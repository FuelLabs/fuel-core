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
    sealed_block_headers: DashMap<Range<u32>, Vec<SealedBlockHeader>>,
    transactions_on_blocks: DashMap<Range<u32>, Vec<Transactions>>,
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
        if let Some(headers) = self.sealed_block_headers.get(&block_height_range) {
            self.update_metrics(increment_p2p_req_res_cache_hits);
            Ok(Some(headers.clone()))
        } else {
            self.update_metrics(increment_p2p_req_res_cache_misses);
            let headers = view.get_sealed_headers(block_height_range.clone())?;
            if let Some(headers) = &headers {
                self.sealed_block_headers
                    .insert(block_height_range, headers.clone());
            }
            Ok(headers)
        }
    }

    pub(crate) fn get_transactions<V>(
        &self,
        view: &V,
        block_height_range: Range<u32>,
    ) -> StorageResult<Option<Vec<Transactions>>>
    where
        V: P2pDb,
    {
        if let Some(transactions) = self.transactions_on_blocks.get(&block_height_range) {
            self.update_metrics(increment_p2p_req_res_cache_hits);
            Ok(Some(transactions.clone()))
        } else {
            self.update_metrics(increment_p2p_req_res_cache_misses);
            let transactions = view.get_transactions(block_height_range.clone())?;
            if let Some(transactions) = &transactions {
                self.transactions_on_blocks
                    .insert(block_height_range, transactions.clone());
            }
            Ok(transactions)
        }
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

        let block_height_range = 0..10;
        let sealed_headers = vec![SealedBlockHeader::default()];
        cached_view
            .sealed_block_headers
            .insert(block_height_range.clone(), sealed_headers.clone());

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
        let block_height_range = 0..10;
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
        let block_height_range = 0..10;
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

        let block_height_range = 0..10;
        let transactions = default_transactions(block_height_range.clone());
        cached_view
            .transactions_on_blocks
            .insert(block_height_range.clone(), transactions.clone());

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
        let block_height_range = 0..10;
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
        let block_height_range = 0..10;
        let result = cached_view
            .get_transactions(&db, block_height_range.clone())
            .unwrap();

        // then
        notified.await;
        assert!(result.is_none());
    }
}
