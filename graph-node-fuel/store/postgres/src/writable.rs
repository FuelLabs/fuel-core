use std::collections::BTreeSet;
use std::ops::Deref;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, RwLock, TryLockError as RwLockError};
use std::time::{Duration, Instant};
use std::{collections::BTreeMap, sync::Arc};

use graph::blockchain::block_stream::FirehoseCursor;
use graph::components::store::{Batch, DeploymentCursorTracker, DerivedEntityQuery, ReadStore};
use graph::constraint_violation;
use graph::data::store::IdList;
use graph::data::subgraph::schema;
use graph::data_source::CausalityRegion;
use graph::prelude::{
    BlockNumber, CacheWeight, Entity, MetricsRegistry, SubgraphDeploymentEntity,
    SubgraphStore as _, BLOCK_NUMBER_MAX,
};
use graph::schema::{EntityKey, EntityType, InputSchema};
use graph::slog::{info, warn};
use graph::tokio::select;
use graph::tokio::sync::Notify;
use graph::tokio::task::JoinHandle;
use graph::util::bounded_queue::BoundedQueue;
use graph::{
    cheap_clone::CheapClone,
    components::store::{self, write::EntityOp, WritableStore as WritableStoreTrait},
    data::subgraph::schema::SubgraphError,
    prelude::{
        BlockPtr, DeploymentHash, EntityModification, Error, Logger, StopwatchMetrics, StoreError,
        StoreEvent, UnfailOutcome, ENV_VARS,
    },
    slog::error,
};
use store::StoredDynamicDataSource;

use crate::deployment_store::DeploymentStore;
use crate::primary::DeploymentId;
use crate::retry;
use crate::{primary, primary::Site, relational::Layout, SubgraphStore};

/// A wrapper around `SubgraphStore` that only exposes functions that are
/// safe to call from `WritableStore`, i.e., functions that either do not
/// deal with anything that depends on a specific deployment
/// location/instance, or where the result is independent of the deployment
/// instance
struct WritableSubgraphStore(SubgraphStore);

impl WritableSubgraphStore {
    fn primary_conn(&self) -> Result<primary::Connection, StoreError> {
        self.0.primary_conn()
    }

    pub(crate) fn send_store_event(&self, event: &StoreEvent) -> Result<(), StoreError> {
        self.0.send_store_event(event)
    }

    fn layout(&self, id: &DeploymentHash) -> Result<Arc<Layout>, StoreError> {
        self.0.layout(id)
    }

    fn load_deployment(&self, site: Arc<Site>) -> Result<SubgraphDeploymentEntity, StoreError> {
        self.0.load_deployment(site)
    }

    fn find_site(&self, id: DeploymentId) -> Result<Arc<Site>, StoreError> {
        self.0.find_site(id)
    }
}

/// Write synchronously to the actual store, i.e., once a method returns,
/// the changes have been committed to the store and are visible to anybody
/// else connecting to that database
struct SyncStore {
    logger: Logger,
    store: WritableSubgraphStore,
    writable: Arc<DeploymentStore>,
    site: Arc<Site>,
    input_schema: InputSchema,
    manifest_idx_and_name: Arc<Vec<(u32, String)>>,
}

impl SyncStore {
    fn new(
        subgraph_store: SubgraphStore,
        logger: Logger,
        site: Arc<Site>,
        manifest_idx_and_name: Arc<Vec<(u32, String)>>,
    ) -> Result<Self, StoreError> {
        let store = WritableSubgraphStore(subgraph_store.clone());
        let writable = subgraph_store.for_site(site.as_ref())?.clone();
        let input_schema = subgraph_store.input_schema(&site.deployment)?;
        Ok(Self {
            logger,
            store,
            writable,
            site,
            input_schema,
            manifest_idx_and_name,
        })
    }

    /// Try to send a `StoreEvent`; if sending fails, log the error but
    /// return `Ok(())`
    fn try_send_store_event(&self, event: StoreEvent) -> Result<(), StoreError> {
        if !ENV_VARS.store.disable_subscription_notifications {
            let _ = self.store.send_store_event(&event).map_err(
                |e| error!(self.logger, "Could not send store event"; "error" => e.to_string()),
            );
            Ok(())
        } else {
            Ok(())
        }
    }
}

// Methods that mirror `WritableStoreTrait`
impl SyncStore {
    async fn block_ptr(&self) -> Result<Option<BlockPtr>, StoreError> {
        retry::forever_async(&self.logger, "block_ptr", || {
            let site = self.site.clone();
            async move { self.writable.block_ptr(site).await }
        })
        .await
    }

    async fn block_cursor(&self) -> Result<FirehoseCursor, StoreError> {
        self.writable
            .block_cursor(self.site.cheap_clone())
            .await
            .map(FirehoseCursor::from)
    }

    fn start_subgraph_deployment(&self, logger: &Logger) -> Result<(), StoreError> {
        retry::forever(&self.logger, "start_subgraph_deployment", || {
            let graft_base = match self.writable.graft_pending(&self.site.deployment)? {
                Some((base_id, base_ptr)) => {
                    let src = self.store.layout(&base_id)?;
                    let deployment_entity = self.store.load_deployment(src.site.clone())?;
                    Some((src, base_ptr, deployment_entity))
                }
                None => None,
            };
            self.writable
                .start_subgraph(logger, self.site.clone(), graft_base)?;
            self.store.primary_conn()?.copy_finished(self.site.as_ref())
        })
    }

    fn revert_block_operations(
        &self,
        block_ptr_to: BlockPtr,
        firehose_cursor: &FirehoseCursor,
    ) -> Result<(), StoreError> {
        retry::forever(&self.logger, "revert_block_operations", || {
            let event = self.writable.revert_block_operations(
                self.site.clone(),
                block_ptr_to.clone(),
                firehose_cursor,
            )?;

            self.try_send_store_event(event)
        })
    }

    fn unfail_deterministic_error(
        &self,
        current_ptr: &BlockPtr,
        parent_ptr: &BlockPtr,
    ) -> Result<UnfailOutcome, StoreError> {
        retry::forever(&self.logger, "unfail_deterministic_error", || {
            self.writable
                .unfail_deterministic_error(self.site.clone(), current_ptr, parent_ptr)
        })
    }

    fn unfail_non_deterministic_error(
        &self,
        current_ptr: &BlockPtr,
    ) -> Result<UnfailOutcome, StoreError> {
        retry::forever(&self.logger, "unfail_non_deterministic_error", || {
            self.writable
                .unfail_non_deterministic_error(self.site.clone(), current_ptr)
        })
    }

    async fn fail_subgraph(&self, error: SubgraphError) -> Result<(), StoreError> {
        retry::forever_async(&self.logger, "fail_subgraph", || {
            let error = error.clone();
            async {
                self.writable
                    .clone()
                    .fail_subgraph(self.site.deployment.clone(), error)
                    .await
            }
        })
        .await
    }

    async fn supports_proof_of_indexing(&self) -> Result<bool, StoreError> {
        retry::forever_async(&self.logger, "supports_proof_of_indexing", || async {
            self.writable
                .supports_proof_of_indexing(self.site.clone())
                .await
        })
        .await
    }

    fn get(&self, key: &EntityKey, block: BlockNumber) -> Result<Option<Entity>, StoreError> {
        retry::forever(&self.logger, "get", || {
            self.writable.get(self.site.cheap_clone(), key, block)
        })
    }

    fn transact_block_operations(
        &self,
        batch: &Batch,
        stopwatch: &StopwatchMetrics,
    ) -> Result<(), StoreError> {
        retry::forever(&self.logger, "transact_block_operations", move || {
            let event = self.writable.transact_block_operations(
                &self.logger,
                self.site.clone(),
                batch,
                stopwatch,
                &self.manifest_idx_and_name,
            )?;

            let _section = stopwatch.start_section("send_store_event");
            self.try_send_store_event(event)?;
            Ok(())
        })
    }

    fn get_many(
        &self,
        keys: BTreeSet<EntityKey>,
        block: BlockNumber,
    ) -> Result<BTreeMap<EntityKey, Entity>, StoreError> {
        let mut by_type: BTreeMap<(EntityType, CausalityRegion), IdList> = BTreeMap::new();
        for key in keys {
            let id_type = key.entity_type.id_type()?;
            by_type
                .entry((key.entity_type, key.causality_region))
                .or_insert_with(|| IdList::new(id_type))
                .push(key.entity_id)?;
        }

        retry::forever(&self.logger, "get_many", || {
            self.writable
                .get_many(self.site.cheap_clone(), &by_type, block)
        })
    }

    fn get_derived(
        &self,
        key: &DerivedEntityQuery,
        block: BlockNumber,
        excluded_keys: Vec<EntityKey>,
    ) -> Result<BTreeMap<EntityKey, Entity>, StoreError> {
        retry::forever(&self.logger, "get_derived", || {
            self.writable
                .get_derived(self.site.cheap_clone(), key, block, &excluded_keys)
        })
    }

    async fn is_deployment_synced(&self) -> Result<bool, StoreError> {
        retry::forever_async(&self.logger, "is_deployment_synced", || async {
            self.writable
                .exists_and_synced(self.site.deployment.cheap_clone())
                .await
        })
        .await
    }

    fn unassign_subgraph(&self, site: &Site) -> Result<(), StoreError> {
        retry::forever(&self.logger, "unassign_subgraph", || {
            let pconn = self.store.primary_conn()?;
            pconn.transaction(|| -> Result<_, StoreError> {
                let changes = pconn.unassign_subgraph(site)?;
                self.store.send_store_event(&StoreEvent::new(changes))
            })
        })
    }

    async fn load_dynamic_data_sources(
        &self,
        block: BlockNumber,
        manifest_idx_and_name: Vec<(u32, String)>,
    ) -> Result<Vec<StoredDynamicDataSource>, StoreError> {
        retry::forever_async(&self.logger, "load_dynamic_data_sources", || async {
            self.writable
                .load_dynamic_data_sources(
                    self.site.cheap_clone(),
                    block,
                    manifest_idx_and_name.clone(),
                )
                .await
        })
        .await
    }

    pub(crate) async fn causality_region_curr_val(
        &self,
    ) -> Result<Option<CausalityRegion>, StoreError> {
        retry::forever_async(&self.logger, "causality_region_curr_val", || async {
            self.writable
                .causality_region_curr_val(self.site.cheap_clone())
                .await
        })
        .await
    }

    fn maybe_find_site(&self, src: DeploymentId) -> Result<Option<Arc<Site>>, StoreError> {
        match self.store.find_site(src) {
            Ok(site) => Ok(Some(site)),
            Err(StoreError::DeploymentNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn deployment_synced(&self) -> Result<(), StoreError> {
        retry::forever(&self.logger, "deployment_synced", || {
            let event = {
                // Make sure we drop `pconn` before we call into the deployment
                // store so that we do not hold two database connections which
                // might come from the same pool and could therefore deadlock
                let pconn = self.store.primary_conn()?;
                pconn.transaction(|| -> Result<_, Error> {
                    let changes = pconn.promote_deployment(&self.site.deployment)?;
                    Ok(StoreEvent::new(changes))
                })?
            };

            // Handle on_sync actions. They only apply to copies (not
            // grafts) so we make sure that the source, if it exists, has
            // the same hash as `self.site`
            if let Some(src) = self.writable.source_of_copy(&self.site)? {
                if let Some(src) = self.maybe_find_site(src)? {
                    if src.deployment == self.site.deployment {
                        let on_sync = self.writable.on_sync(&self.site)?;
                        if on_sync.activate() {
                            let pconn = self.store.primary_conn()?;
                            pconn.activate(&self.site.as_ref().into())?;
                        }
                        if on_sync.replace() {
                            self.unassign_subgraph(&src)?;
                        }
                    }
                }
            }

            self.writable.deployment_synced(&self.site.deployment)?;

            self.store.send_store_event(&event)
        })
    }

    fn shard(&self) -> &str {
        self.site.shard.as_str()
    }

    async fn health(&self) -> Result<schema::SubgraphHealth, StoreError> {
        retry::forever_async(&self.logger, "health", || async {
            self.writable.health(&self.site).await.map(Into::into)
        })
        .await
    }

    fn input_schema(&self) -> InputSchema {
        self.input_schema.cheap_clone()
    }
}

/// Track block numbers we see in a few methods that traverse the queue to
/// help determine which changes in the queue will actually be visible in
/// the database once the whole queue has been processed and the block
/// number at which queries should run so that they only consider data that
/// is not affected by any requests currently queued.
///
/// The best way to use the tracker is to use the `fold_map` and `find`
/// methods.
#[derive(Debug)]
struct BlockTracker {
    /// The smallest block number that has been reverted to. Only writes
    /// before this block will be visible
    revert: BlockNumber,
    /// The largest block number that is not affected by entries in the
    /// queue
    block: BlockNumber,
}

impl BlockTracker {
    fn new() -> Self {
        Self {
            revert: BLOCK_NUMBER_MAX,
            block: BLOCK_NUMBER_MAX,
        }
    }

    fn write(&mut self, block_ptr: &BlockPtr) {
        self.block = self.block.min(block_ptr.number - 1);
    }

    fn revert(&mut self, block_ptr: &BlockPtr) {
        // `block_ptr` is the block pointer we are reverting _to_,
        // and is not affected by the revert
        self.revert = self.revert.min(block_ptr.number);
        self.block = self.block.min(block_ptr.number);
    }

    /// The block at which a query should run so it does not see the result
    /// of any writes that might have happened concurrently but have already
    /// been accounted for by inspecting the in-memory queue
    fn query_block(&self) -> BlockNumber {
        self.block
    }

    /// Iterate over all batches currently in the queue, from newest to
    /// oldest, and call `f` for each batch whose changes will actually be
    /// visible in the database once the entire queue has been processed.
    ///
    /// The iteration ends the first time that `f` returns `Some(_)`. The
    /// queue will be locked during the iteration, so `f` should not do any
    /// slow work.
    ///
    /// The returned `BlockNumber` is the block at which queries should run
    /// to only consider the state of the database before any of the queued
    /// changes have been applied.
    fn find_map<R, F>(queue: &BoundedQueue<Arc<Request>>, f: F) -> (Option<R>, BlockNumber)
    where
        F: Fn(&Batch, BlockNumber) -> Option<R>,
    {
        let mut tracker = BlockTracker::new();
        // Going from newest to oldest entry in the queue as `find_map` does
        // ensures that we see reverts before we see the corresponding write
        // request. We ignore any write request that writes blocks that have
        // a number strictly higher than the revert with the smallest block
        // number, as all such writes will be undone once the revert is
        // processed.
        let res = queue.find_map(|req| match req.as_ref() {
            Request::Write { batch, .. } => {
                let batch = batch.read().unwrap();
                tracker.write(&batch.block_ptr);
                if batch.first_block <= tracker.revert {
                    let res = f(batch.deref(), tracker.revert);
                    if res.is_some() {
                        return res;
                    }
                }
                None
            }
            Request::RevertTo { block_ptr, .. } => {
                tracker.revert(block_ptr);
                None
            }
            Request::Stop => None,
        });
        (res, tracker.query_block())
    }

    /// Iterate over all batches currently in the queue, from newest to
    /// oldest, and call `f` for each batch whose changes will actually be
    /// visible in the database once the entire queue has been processed.
    ///
    /// Return the value that the last invocation of `f` returned, together
    /// with the block at which queries should run to only consider the
    /// state of the database before any of the queued changes have been
    /// applied.
    ///
    /// The queue will be locked during the iteration, so `f` should not do
    /// any slow work.
    fn fold<F, B>(queue: &BoundedQueue<Arc<Request>>, init: B, mut f: F) -> (B, BlockNumber)
    where
        F: FnMut(B, &Batch, BlockNumber) -> B,
    {
        let mut tracker = BlockTracker::new();

        let accum = queue.fold(init, |accum, req| {
            match req.as_ref() {
                Request::Write { batch, .. } => {
                    let batch = batch.read().unwrap();
                    let mut accum = accum;
                    tracker.write(&batch.block_ptr);
                    if batch.first_block <= tracker.revert {
                        accum = f(accum, batch.deref(), tracker.revert);
                    }
                    accum
                }
                Request::RevertTo { block_ptr, .. } => {
                    tracker.revert(block_ptr);
                    accum
                }
                Request::Stop => {
                    /* nothing to do */
                    accum
                }
            }
        });
        (accum, tracker.query_block())
    }
}

/// A write request received from the `WritableStore` frontend that gets
/// queued
///
/// The `processed` flag is set to true as soon as the background writer is
/// working on that request. Once it has been set, no changes can be made to
/// the request
enum Request {
    Write {
        queued: Instant,
        store: Arc<SyncStore>,
        stopwatch: StopwatchMetrics,
        // The batch is in a `RwLock` because `push_write` will try to add
        // to the batch under the right conditions, and other operations
        // will try to read the batch. The batch only becomes truly readonly
        // when we decide to process it at which point we set `processed` to
        // `true`
        batch: RwLock<Batch>,
        processed: AtomicBool,
    },
    RevertTo {
        store: Arc<SyncStore>,
        /// The subgraph head will be at this block pointer after the revert
        block_ptr: BlockPtr,
        firehose_cursor: FirehoseCursor,
        processed: AtomicBool,
    },
    Stop,
}

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Write { batch, store, .. } => {
                let batch = batch.read().unwrap();
                write!(
                    f,
                    "write[{}, {:p}, {} entities]",
                    batch.block_ptr.number,
                    store.as_ref(),
                    batch.entity_count()
                )
            }
            Self::RevertTo {
                block_ptr, store, ..
            } => write!(f, "revert[{}, {:p}]", block_ptr.number, store.as_ref()),
            Self::Stop => write!(f, "stop"),
        }
    }
}

enum ExecResult {
    Continue,
    Stop,
}

impl Request {
    fn write(store: Arc<SyncStore>, stopwatch: StopwatchMetrics, batch: Batch) -> Self {
        Self::Write {
            queued: Instant::now(),
            store,
            stopwatch,
            batch: RwLock::new(batch),
            processed: AtomicBool::new(false),
        }
    }

    fn revert(store: Arc<SyncStore>, block_ptr: BlockPtr, firehose_cursor: FirehoseCursor) -> Self {
        Self::RevertTo {
            store,
            block_ptr,
            firehose_cursor,
            processed: AtomicBool::new(false),
        }
    }

    fn start_process(&self) {
        match self {
            Request::Write { processed, .. } | Request::RevertTo { processed, .. } => {
                processed.store(true, Ordering::SeqCst)
            }
            Request::Stop => { /* nothing to do  */ }
        }
    }

    fn processed(&self) -> bool {
        match self {
            Request::Write { processed, .. } | Request::RevertTo { processed, .. } => {
                processed.load(Ordering::SeqCst)
            }
            Request::Stop => false,
        }
    }

    fn execute(&self) -> Result<ExecResult, StoreError> {
        match self {
            Request::Write {
                batch,
                store,
                stopwatch,
                queued: _,
                processed: _,
            } => {
                let start = Instant::now();
                let batch = batch.read().unwrap();
                if let Some(err) = &batch.error {
                    // This can happen when appending to the batch failed
                    // because of a constraint violation. Returning an `Err`
                    // here will poison and shut down the queue
                    return Err(err.clone());
                }
                let res = store
                    .transact_block_operations(batch.deref(), stopwatch)
                    .map(|()| ExecResult::Continue);
                info!(store.logger, "Committed write batch";
                        "block_number" => batch.block_ptr.number,
                        "block_count" => batch.block_ptr.number - batch.first_block + 1,
                        "entities" => batch.entity_count(),
                        "weight" => batch.weight(),
                        "time_ms" => start.elapsed().as_millis());
                res
            }
            Request::RevertTo {
                store,
                block_ptr,
                firehose_cursor,
                processed: _,
            } => store
                .revert_block_operations(block_ptr.clone(), firehose_cursor)
                .map(|()| ExecResult::Continue),
            Request::Stop => Ok(ExecResult::Stop),
        }
    }

    /// Return `true` if we should process this request right away. Return
    /// `false` if we should wait for a little longer with processing the
    /// request
    fn should_process(&self) -> bool {
        match self {
            Request::Write { queued, batch, .. } => {
                batch.read().unwrap().weight() >= ENV_VARS.store.write_batch_size
                    || queued.elapsed() >= ENV_VARS.store.write_batch_duration
            }
            Request::RevertTo { .. } | Request::Stop => true,
        }
    }

    fn is_write(&self) -> bool {
        match self {
            Request::Write { .. } => true,
            Request::RevertTo { .. } | Request::Stop => false,
        }
    }
}

/// A queue that asynchronously writes requests queued with `push` to the
/// underlying store and allows retrieving information that is a combination
/// of queued changes and changes already committed to the store.
struct Queue {
    store: Arc<SyncStore>,
    /// A queue of pending requests. New requests are appended at the back,
    /// and popped off the front for processing. When the queue only
    /// contains `Write` requests block numbers in the requests are
    /// increasing going front-to-back. When `Revert` requests are queued,
    /// that is not true anymore
    queue: BoundedQueue<Arc<Request>>,

    /// The write task puts errors from `transact_block_operations` here so
    /// we can report them on the next call to transact block operations.
    write_err: Mutex<Option<StoreError>>,

    /// True if the background worker ever encountered an error. Once that
    /// happens, no more changes will be written, and any attempt to write
    /// or revert will result in an error
    poisoned: AtomicBool,

    stopwatch: StopwatchMetrics,

    /// Wether we should attempt to combine writes into large batches
    /// spanning multiple blocks. This is initially `true` and gets set to
    /// `false` when the subgraph is marked as synced.
    batch_writes: AtomicBool,

    /// Notify the background writer as soon as we are told to stop
    /// batching or there is a batch that is big enough to proceed.
    batch_ready_notify: Arc<Notify>,
}

/// Support for controlling the background writer (pause/resume) only for
/// use in tests. In release builds, the checks that pause the writer are
/// compiled out. Before `allow_steps` is called, the background writer is
/// allowed to process as many requests as it can
#[cfg(debug_assertions)]
pub(crate) mod test_support {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use graph::{
        components::store::{DeploymentId, DeploymentLocator},
        prelude::lazy_static,
        util::bounded_queue::BoundedQueue,
    };

    lazy_static! {
        static ref STEPS: Mutex<HashMap<DeploymentId, Arc<BoundedQueue<()>>>> =
            Mutex::new(HashMap::new());
    }

    pub(super) async fn take_step(deployment: &DeploymentLocator) {
        let steps = STEPS.lock().unwrap().get(&deployment.id).cloned();
        if let Some(steps) = steps {
            steps.pop().await;
        }
    }

    /// Allow the writer to process `steps` requests. After calling this,
    /// the writer will only process the number of requests it is allowed to
    pub async fn allow_steps(deployment: &DeploymentLocator, steps: usize) {
        let queue = {
            let mut map = STEPS.lock().unwrap();
            map.entry(deployment.id)
                .or_insert_with(|| Arc::new(BoundedQueue::with_capacity(1_000)))
                .clone()
        };
        for _ in 0..steps {
            queue.push(()).await
        }
    }

    pub async fn flush_steps(deployment: graph::components::store::DeploymentId) {
        let queue = {
            let mut map = STEPS.lock().unwrap();
            map.remove(&deployment)
        };
        if let Some(queue) = queue {
            queue.push(()).await;
        }
    }
}

impl std::fmt::Debug for Queue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let reqs = self.queue.fold(vec![], |mut reqs, req| {
            reqs.push(req.clone());
            reqs
        });

        write!(f, "reqs[{} : ", self.store.site)?;
        for req in reqs {
            write!(f, " {:?}", req)?;
        }
        writeln!(f, "]")
    }
}

impl Queue {
    /// Create a new queue and spawn a task that processes write requests
    fn start(
        logger: Logger,
        store: Arc<SyncStore>,
        capacity: usize,
        registry: Arc<MetricsRegistry>,
    ) -> (Arc<Self>, JoinHandle<()>) {
        async fn start_writer(queue: Arc<Queue>, logger: Logger, batch_stop_notify: Arc<Notify>) {
            loop {
                #[cfg(debug_assertions)]
                test_support::take_step(&queue.store.site.as_ref().into()).await;

                // If batching is enabled, hold off on writing a batch for a
                // little bit to give processing a chance to add more
                // changes. We start processing a batch if it is big enough
                // or old enough, or if there is more than one request in
                // the queue. The latter condition makes sure that we do not
                // wait for a batch to grow when `push_write` would never
                // add to it again.
                if queue.batch_writes() && queue.queue.len() <= 1 {
                    loop {
                        let _section = queue.stopwatch.start_section("queue_wait");
                        let req = queue.queue.peek().await;

                        // When this is true, push_write would never add to
                        // `req`, and we therefore execute the request as
                        // waiting for more changes to it would be pointless
                        if !queue.batch_writes() || queue.queue.len() > 1 || req.should_process() {
                            break;
                        }

                        // Wait until something has changed before checking
                        // again, either because we were notified that the
                        // batch should be processed or after some time
                        // passed. The latter is just for safety in case
                        // there is a mistake with notifications.
                        let sleep = graph::tokio::time::sleep(Duration::from_secs(2));
                        let notify = batch_stop_notify.notified();
                        select!(
                            () = sleep => (),
                            () = notify => (),
                        );
                    }
                }

                // We peek at the front of the queue, rather than pop it
                // right away, so that query methods like `get` have access
                // to the data while it is being written. If we popped here,
                // these methods would not be able to see that data until
                // the write transaction commits, causing them to return
                // incorrect results.
                let req = {
                    let _section = queue.stopwatch.start_section("queue_wait");
                    // Mark the request as being processed so push_write
                    // will not modify it again, even after we are done with
                    // it here
                    queue.queue.peek_with(|req| req.start_process()).await
                };
                let res = {
                    let _section = queue.stopwatch.start_section("queue_execute");
                    graph::spawn_blocking_allow_panic(move || req.execute()).await
                };

                let _section = queue.stopwatch.start_section("queue_pop");
                use ExecResult::*;
                match res {
                    Ok(Ok(Continue)) => {
                        // The request has been handled. It's now safe to remove it
                        // from the queue
                        queue.queue.pop().await;
                    }
                    Ok(Ok(Stop)) => {
                        // Graceful shutdown. We also handled the request
                        // successfully
                        queue.queue.pop().await;
                        return;
                    }
                    Ok(Err(e)) => {
                        error!(logger, "Subgraph writer failed"; "error" => e.to_string());
                        queue.record_err(e);
                        return;
                    }
                    Err(e) => {
                        error!(logger, "Subgraph writer paniced"; "error" => e.to_string());
                        queue.record_err(StoreError::WriterPanic(e));
                        return;
                    }
                }
            }
        }

        let queue = BoundedQueue::with_capacity(capacity);
        let write_err = Mutex::new(None);

        // Use a separate instance of the `StopwatchMetrics` for background
        // work since that has its own call hierarchy, and using the
        // foreground metrics will lead to incorrect nesting of sections
        let stopwatch = StopwatchMetrics::new(
            logger.clone(),
            store.site.deployment.clone(),
            "writer",
            registry,
            store.shard().to_string(),
        );

        let batch_ready_notify = Arc::new(Notify::new());
        let queue = Self {
            store,
            queue,
            write_err,
            poisoned: AtomicBool::new(false),
            stopwatch,
            batch_writes: AtomicBool::new(true),
            batch_ready_notify: batch_ready_notify.clone(),
        };
        let queue = Arc::new(queue);

        let handle = graph::spawn(start_writer(
            queue.cheap_clone(),
            logger,
            batch_ready_notify,
        ));

        (queue, handle)
    }

    /// Add a write request to the queue
    async fn push(&self, req: Request) -> Result<(), StoreError> {
        self.check_err()?;
        // If we see anything but a write we have to turn off batching as
        // that would risk adding changes from after a revert into a batch
        // that gets processed before the revert
        if !req.is_write() {
            self.stop_batching();
        }
        self.queue.push(Arc::new(req)).await;
        Ok(())
    }

    /// Try to append the `batch` to the newest request in the queue if that
    /// is a write request. We will only append if several conditions are
    /// true:
    ///
    ///   1. The subgraph is not synced
    ///   2. The newest request (back of the queue) is a write
    ///   3. The newest request is not already being processed by the
    ///      writing thread
    ///   4. The newest write request is not older than
    ///      `GRAPH_STORE_WRITE_BATCH_DURATION`
    ///   5. The newest write request is not bigger than
    ///      `GRAPH_STORE_WRITE_BATCH_SIZE`
    ///
    /// In all other cases, we queue a new write request. Note that (3)
    /// means that the oldest request (front of the queue) does not
    /// necessarily fulfill (4) and (5) even if it is a write and the
    /// subgraph is not synced yet.
    ///
    /// This strategy is closely tied to how start_writer waits for writes
    /// to fill up before writing them to maximize the chances that we build
    /// a 'full' write batch, i.e., one that is either big enough or old
    /// enough
    async fn push_write(&self, batch: Batch) -> Result<(), StoreError> {
        let batch = if ENV_VARS.store.write_batch_size == 0
            || ENV_VARS.store.write_batch_duration.is_zero()
            || !self.batch_writes()
        {
            Some(batch)
        } else {
            self.queue.map_newest(move |newest| {
                let newest = match newest {
                    Some(newest) => newest,
                    None => {
                        return Ok(Some(batch));
                    }
                };
                // This check at first seems redundant with getting the lock
                // on the batch in the request below, but is very important
                // for correctness: if the writer has finished processing
                // the request and released its lock on the batch, without
                // this check, we would modify a request that has already
                // been written, and our changes would therefore never be
                // written
                if newest.processed() {
                    return Ok(Some(batch));
                }
                match newest.as_ref() {
                    Request::Write {
                        batch: existing,
                        queued,
                        ..
                    } => {
                        if queued.elapsed() < ENV_VARS.store.write_batch_duration {
                            // We are being very defensive here: if anything
                            // is holding the lock on the batch, do not
                            // modify it. We create a new request instead of
                            // waiting for the lock since writing a batch
                            // holds a read lock on the batch for the
                            // duration of the write, and we do not want to
                            // slow down queueing requests unnecessarily
                            match existing.try_write() {
                                Ok(mut existing) => {
                                    if existing.weight() < ENV_VARS.store.write_batch_size {
                                        let res = existing.append(batch).map(|()| None);
                                        if existing.weight() >= ENV_VARS.store.write_batch_size {
                                            self.batch_ready_notify.notify_one();
                                        }
                                        res
                                    } else {
                                        Ok(Some(batch))
                                    }
                                }
                                Err(RwLockError::WouldBlock) => {
                                    // This branch can cause batches that
                                    // are not 'full' at the head of the
                                    // queue, something that start_writer
                                    // has to take into account
                                    return Ok(Some(batch));
                                }
                                Err(RwLockError::Poisoned(e)) => {
                                    panic!("rwlock on batch was poisoned {:?}", e);
                                }
                            }
                        } else {
                            Ok(Some(batch))
                        }
                    }
                    Request::RevertTo { .. } | Request::Stop => Ok(Some(batch)),
                }
            })?
        };

        if let Some(batch) = batch {
            let req = Request::write(
                self.store.cheap_clone(),
                self.stopwatch.cheap_clone(),
                batch,
            );
            self.push(req).await?;
        }
        Ok(())
    }

    /// Wait for the background writer to finish processing queued entries
    async fn flush(&self) -> Result<(), StoreError> {
        self.check_err()?;

        #[cfg(debug_assertions)]
        test_support::flush_steps(self.store.site.id.into()).await;

        // Turn off batching so the queue doesn't wait for a batch to become
        // full, but restore the old behavior once the queue is empty.
        let batching = self.batch_writes.load(Ordering::SeqCst);
        self.stop_batching();

        self.queue.wait_empty().await;

        self.batch_writes.store(batching, Ordering::SeqCst);
        self.check_err()
    }

    async fn stop(&self) -> Result<(), StoreError> {
        self.stop_batching();
        self.push(Request::Stop).await
    }

    fn check_err(&self) -> Result<(), StoreError> {
        if let Some(err) = self.write_err.lock().unwrap().take() {
            return Err(err);
        }
        match self.poisoned.load(Ordering::SeqCst) {
            true => Err(StoreError::Poisoned),
            false => Ok(()),
        }
    }

    /// Record the error `e`, mark the queue as poisoned, and remove all
    /// pending requests. The queue can not be used anymore
    fn record_err(&self, e: StoreError) {
        *self.write_err.lock().unwrap() = Some(e);
        self.poisoned.store(true, Ordering::SeqCst);
        self.queue.clear();
    }

    /// Get the entity for `key` if it exists by looking at both the queue
    /// and the store
    fn get(&self, key: &EntityKey) -> Result<Option<Entity>, StoreError> {
        enum Op {
            Write(Entity),
            Remove,
        }

        impl<'a> From<EntityOp<'a>> for Op {
            fn from(value: EntityOp) -> Self {
                match value {
                    EntityOp::Write { key: _, entity } => Self::Write(entity.clone()),
                    EntityOp::Remove { .. } => Self::Remove,
                }
            }
        }

        let (op, query_block) = BlockTracker::find_map(&self.queue, |batch, at| {
            batch.last_op(key, at).map(Op::from)
        });

        match op {
            Some(Op::Write(entity)) => Ok(Some(entity)),
            Some(Op::Remove) => Ok(None),
            None => self.store.get(key, query_block),
        }
    }

    /// Get many entities at once by looking at both the queue and the store
    fn get_many(
        &self,
        mut keys: BTreeSet<EntityKey>,
    ) -> Result<BTreeMap<EntityKey, Entity>, StoreError> {
        let (entities_in_queue, query_block) = BlockTracker::fold(
            &self.queue,
            BTreeMap::new(),
            |mut map: BTreeMap<EntityKey, Option<Entity>>, batch, at| {
                // See if we have changes for any of the keys. Since we are
                // going from newest to oldest block, do not clobber already
                // existing entries in map as that would make us use an
                // older value.
                for key in &keys {
                    if map.contains_key(key) {
                        continue;
                    }
                    match batch.last_op(key, at) {
                        Some(EntityOp::Write { key: _, entity }) => {
                            map.insert(key.clone(), Some(entity.clone()));
                        }
                        Some(EntityOp::Remove { .. }) => {
                            map.insert(key.clone(), None);
                        }
                        None => { /* nothing to do  */ }
                    }
                }
                map
            },
        );

        // Look entities for the remaining keys up in the store
        keys.retain(|key| !entities_in_queue.contains_key(key));
        let mut map = self.store.get_many(keys, query_block)?;

        // Extend the store results with the entities from the queue.
        for (key, entity) in entities_in_queue {
            if let Some(entity) = entity {
                let overwrite = map.insert(key, entity).is_some();
                assert!(!overwrite);
            }
        }

        Ok(map)
    }

    fn get_derived(
        &self,
        derived_query: &DerivedEntityQuery,
    ) -> Result<BTreeMap<EntityKey, Entity>, StoreError> {
        fn is_related(derived_query: &DerivedEntityQuery, entity: &Entity) -> bool {
            entity
                .get(&derived_query.entity_field)
                .map(|v| &derived_query.value == v)
                .unwrap_or(false)
        }

        fn effective_ops<'a>(
            batch: &'a Batch,
            derived_query: &'a DerivedEntityQuery,
            at: BlockNumber,
        ) -> impl Iterator<Item = (EntityKey, Option<Entity>)> + 'a {
            batch
                .effective_ops(&derived_query.entity_type, at)
                .filter_map(|op| match op {
                    EntityOp::Write { key, entity } if is_related(derived_query, entity) => {
                        Some((key.clone(), Some(entity.clone())))
                    }
                    EntityOp::Write { .. } => None,
                    EntityOp::Remove { key } => Some((key.clone(), None)),
                })
        }

        // Get entities from entries in the queue
        let (entities_in_queue, query_block) = BlockTracker::fold(
            &self.queue,
            BTreeMap::new(),
            |mut map: BTreeMap<EntityKey, Option<Entity>>, batch, at| {
                // Since we are going newest to oldest, do not clobber
                // already existing entries in map as that would make us
                // produce stale values
                for (k, v) in effective_ops(batch, derived_query, at) {
                    if !map.contains_key(&k) {
                        map.insert(k, v);
                    }
                }
                map
            },
        );

        let excluded_keys: Vec<EntityKey> = entities_in_queue.keys().cloned().collect();

        // We filter to exclude the entities ids that we already have from the queue
        let mut items_from_database =
            self.store
                .get_derived(derived_query, query_block, excluded_keys)?;

        // Extend the store results with the entities from the queue.
        // This overwrites any entitiy from the database with the same key from queue
        let items_from_queue: BTreeMap<EntityKey, Entity> = entities_in_queue
            .into_iter()
            .filter_map(|(key, entity)| entity.map(|entity| (key, entity)))
            .collect();
        items_from_database.extend(items_from_queue);

        Ok(items_from_database)
    }

    /// Load dynamic data sources by looking at both the queue and the store
    async fn load_dynamic_data_sources(
        &self,
        manifest_idx_and_name: Vec<(u32, String)>,
    ) -> Result<Vec<StoredDynamicDataSource>, StoreError> {
        // We need to produce a list of dynamic data sources that are
        // ordered by their creation block. We first look through all the
        // dds that are still in the queue, and then load dds from the store
        // as long as they were written at a block before whatever is still
        // in the queue. The overall list of dds is the list of dds from the
        // store plus the ones still in memory sorted by their block number.
        let (mut queue_dds, query_block) =
            BlockTracker::fold(&self.queue, Vec::new(), |mut dds, batch, at| {
                dds.extend(batch.new_data_sources(at).cloned());
                dds
            });
        // Using a stable sort is important here so that dds created at the
        // same block stay in the order in which they were added (and
        // therefore will be loaded from the store in that order once the
        // queue has been written)
        queue_dds.sort_by_key(|dds| dds.creation_block);

        let mut dds = self
            .store
            .load_dynamic_data_sources(query_block, manifest_idx_and_name)
            .await?;
        dds.append(&mut queue_dds);

        Ok(dds)
    }

    fn poisoned(&self) -> bool {
        self.poisoned.load(Ordering::SeqCst)
    }

    fn deployment_synced(&self) {
        self.stop_batching();
        self.stopwatch.disable()
    }

    fn batch_writes(&self) -> bool {
        self.batch_writes.load(Ordering::SeqCst)
    }

    fn stop_batching(&self) {
        self.batch_writes.store(false, Ordering::SeqCst);
        self.batch_ready_notify.notify_one();
    }
}

/// A shim to allow bypassing any pipelined store handling if need be
enum Writer {
    Sync(Arc<SyncStore>),
    Async {
        queue: Arc<Queue>,
        join_handle: JoinHandle<()>,
    },
}

impl Writer {
    fn new(
        logger: Logger,
        store: Arc<SyncStore>,
        capacity: usize,
        registry: Arc<MetricsRegistry>,
    ) -> Self {
        info!(logger, "Starting subgraph writer"; "queue_size" => capacity);
        if capacity == 0 {
            Self::Sync(store)
        } else {
            let (queue, join_handle) = Queue::start(logger, store.clone(), capacity, registry);
            Self::Async { queue, join_handle }
        }
    }

    fn check_queue_running(&self) -> Result<(), StoreError> {
        match self {
            Writer::Sync(_) => Ok(()),
            Writer::Async { join_handle, queue } => {
                // If there was an error, report that instead of a naked 'writer not running'
                queue.check_err()?;
                if join_handle.is_finished() {
                    Err(constraint_violation!(
                        "Subgraph writer for {} is not running",
                        queue.store.site
                    ))
                } else {
                    Ok(())
                }
            }
        }
    }

    async fn write(&self, batch: Batch, stopwatch: &StopwatchMetrics) -> Result<(), StoreError> {
        const MAX_BATCH_TIME: Duration = Duration::from_secs(30);

        match self {
            Writer::Sync(store) => store.transact_block_operations(&batch, stopwatch),
            Writer::Async { queue, .. } => {
                self.check_queue_running()?;
                queue.push_write(batch).await
            }
        }
    }

    async fn revert(
        &self,
        block_ptr_to: BlockPtr,
        firehose_cursor: FirehoseCursor,
    ) -> Result<(), StoreError> {
        match self {
            Writer::Sync(store) => store.revert_block_operations(block_ptr_to, &firehose_cursor),
            Writer::Async { queue, .. } => {
                self.check_queue_running()?;
                let req = Request::revert(queue.store.cheap_clone(), block_ptr_to, firehose_cursor);
                queue.push(req).await
            }
        }
    }

    async fn flush(&self) -> Result<(), StoreError> {
        match self {
            Writer::Sync { .. } => Ok(()),
            Writer::Async { queue, .. } => {
                self.check_queue_running()?;
                queue.flush().await
            }
        }
    }

    fn get(&self, key: &EntityKey) -> Result<Option<Entity>, StoreError> {
        match self {
            Writer::Sync(store) => store.get(key, BLOCK_NUMBER_MAX),
            Writer::Async { queue, .. } => queue.get(key),
        }
    }

    fn get_many(
        &self,
        keys: BTreeSet<EntityKey>,
    ) -> Result<BTreeMap<EntityKey, Entity>, StoreError> {
        match self {
            Writer::Sync(store) => store.get_many(keys, BLOCK_NUMBER_MAX),
            Writer::Async { queue, .. } => queue.get_many(keys),
        }
    }

    fn get_derived(
        &self,
        key: &DerivedEntityQuery,
    ) -> Result<BTreeMap<EntityKey, Entity>, StoreError> {
        match self {
            Writer::Sync(store) => store.get_derived(key, BLOCK_NUMBER_MAX, vec![]),
            Writer::Async { queue, .. } => queue.get_derived(key),
        }
    }

    async fn load_dynamic_data_sources(
        &self,
        manifest_idx_and_name: Vec<(u32, String)>,
    ) -> Result<Vec<StoredDynamicDataSource>, StoreError> {
        match self {
            Writer::Sync(store) => {
                store
                    .load_dynamic_data_sources(BLOCK_NUMBER_MAX, manifest_idx_and_name)
                    .await
            }
            Writer::Async { queue, .. } => {
                queue.load_dynamic_data_sources(manifest_idx_and_name).await
            }
        }
    }

    fn poisoned(&self) -> bool {
        match self {
            Writer::Sync(_) => false,
            Writer::Async { queue, .. } => queue.poisoned(),
        }
    }

    async fn stop(&self) -> Result<(), StoreError> {
        match self {
            Writer::Sync(_) => Ok(()),
            Writer::Async { queue, .. } => queue.stop().await,
        }
    }

    fn deployment_synced(&self) {
        match self {
            Writer::Sync(_) => {}
            Writer::Async { queue, .. } => queue.deployment_synced(),
        }
    }
}

pub struct WritableStore {
    store: Arc<SyncStore>,
    block_ptr: Mutex<Option<BlockPtr>>,
    block_cursor: Mutex<FirehoseCursor>,
    writer: Writer,
}

impl WritableStore {
    pub(crate) async fn new(
        subgraph_store: SubgraphStore,
        logger: Logger,
        site: Arc<Site>,
        manifest_idx_and_name: Arc<Vec<(u32, String)>>,
        registry: Arc<MetricsRegistry>,
    ) -> Result<Self, StoreError> {
        let store = Arc::new(SyncStore::new(
            subgraph_store,
            logger.clone(),
            site,
            manifest_idx_and_name,
        )?);
        let block_ptr = Mutex::new(store.block_ptr().await?);
        let block_cursor = Mutex::new(store.block_cursor().await?);
        let writer = Writer::new(
            logger,
            store.clone(),
            ENV_VARS.store.write_queue_size,
            registry,
        );

        Ok(Self {
            store,
            block_ptr,
            block_cursor,
            writer,
        })
    }

    pub(crate) fn poisoned(&self) -> bool {
        self.writer.poisoned()
    }

    pub(crate) async fn stop(&self) -> Result<(), StoreError> {
        self.writer.stop().await
    }
}

impl ReadStore for WritableStore {
    fn get(&self, key: &EntityKey) -> Result<Option<Entity>, StoreError> {
        self.writer.get(key)
    }

    fn get_many(
        &self,
        keys: BTreeSet<EntityKey>,
    ) -> Result<BTreeMap<EntityKey, Entity>, StoreError> {
        self.writer.get_many(keys)
    }

    fn get_derived(
        &self,
        key: &DerivedEntityQuery,
    ) -> Result<BTreeMap<EntityKey, Entity>, StoreError> {
        self.writer.get_derived(key)
    }

    fn input_schema(&self) -> InputSchema {
        self.store.input_schema()
    }
}

impl DeploymentCursorTracker for WritableStore {
    fn block_ptr(&self) -> Option<BlockPtr> {
        self.block_ptr.lock().unwrap().clone()
    }

    fn firehose_cursor(&self) -> FirehoseCursor {
        self.block_cursor.lock().unwrap().clone()
    }

    fn input_schema(&self) -> InputSchema {
        self.store.input_schema()
    }
}

#[async_trait::async_trait]
impl WritableStoreTrait for WritableStore {
    async fn start_subgraph_deployment(&self, logger: &Logger) -> Result<(), StoreError> {
        let store = self.store.cheap_clone();
        let logger = logger.cheap_clone();
        graph::spawn_blocking_allow_panic(move || store.start_subgraph_deployment(&logger))
            .await
            .map_err(Error::from)??;

        // Refresh all in memory state in case this instance was used before
        *self.block_ptr.lock().unwrap() = self.store.block_ptr().await?;
        *self.block_cursor.lock().unwrap() = self.store.block_cursor().await?;

        Ok(())
    }

    async fn revert_block_operations(
        &self,
        block_ptr_to: BlockPtr,
        firehose_cursor: FirehoseCursor,
    ) -> Result<(), StoreError> {
        *self.block_ptr.lock().unwrap() = Some(block_ptr_to.clone());
        *self.block_cursor.lock().unwrap() = firehose_cursor.clone();

        self.writer.revert(block_ptr_to, firehose_cursor).await
    }

    async fn unfail_deterministic_error(
        &self,
        current_ptr: &BlockPtr,
        parent_ptr: &BlockPtr,
    ) -> Result<UnfailOutcome, StoreError> {
        let outcome = self
            .store
            .unfail_deterministic_error(current_ptr, parent_ptr)?;

        if let UnfailOutcome::Unfailed = outcome {
            *self.block_ptr.lock().unwrap() = self.store.block_ptr().await?;
            *self.block_cursor.lock().unwrap() = self.store.block_cursor().await?;
        }

        Ok(outcome)
    }

    fn unfail_non_deterministic_error(
        &self,
        current_ptr: &BlockPtr,
    ) -> Result<UnfailOutcome, StoreError> {
        // We don't have to update in memory self.block_ptr
        // because the method call below doesn't rewind/revert
        // any block.
        self.store.unfail_non_deterministic_error(current_ptr)
    }

    async fn fail_subgraph(&self, error: SubgraphError) -> Result<(), StoreError> {
        self.store.fail_subgraph(error).await
    }

    async fn supports_proof_of_indexing(&self) -> Result<bool, StoreError> {
        self.store.supports_proof_of_indexing().await
    }

    async fn transact_block_operations(
        &self,
        block_ptr_to: BlockPtr,
        firehose_cursor: FirehoseCursor,
        mods: Vec<EntityModification>,
        stopwatch: &StopwatchMetrics,
        data_sources: Vec<StoredDynamicDataSource>,
        deterministic_errors: Vec<SubgraphError>,
        processed_data_sources: Vec<StoredDynamicDataSource>,
        is_non_fatal_errors_active: bool,
    ) -> Result<(), StoreError> {
        let batch = Batch::new(
            block_ptr_to.clone(),
            firehose_cursor.clone(),
            mods,
            data_sources,
            deterministic_errors,
            processed_data_sources,
            is_non_fatal_errors_active,
        )?;
        self.writer.write(batch, stopwatch).await?;

        *self.block_ptr.lock().unwrap() = Some(block_ptr_to);
        *self.block_cursor.lock().unwrap() = firehose_cursor;

        Ok(())
    }

    fn deployment_synced(&self) -> Result<(), StoreError> {
        self.writer.deployment_synced();
        self.store.deployment_synced()
    }

    async fn is_deployment_synced(&self) -> Result<bool, StoreError> {
        self.store.is_deployment_synced().await
    }

    fn unassign_subgraph(&self) -> Result<(), StoreError> {
        self.store.unassign_subgraph(&self.store.site)
    }

    async fn load_dynamic_data_sources(
        &self,
        manifest_idx_and_name: Vec<(u32, String)>,
    ) -> Result<Vec<StoredDynamicDataSource>, StoreError> {
        self.writer
            .load_dynamic_data_sources(manifest_idx_and_name)
            .await
    }

    async fn causality_region_curr_val(&self) -> Result<Option<CausalityRegion>, StoreError> {
        // It should be empty when we call this, but just in case.
        self.writer.flush().await?;
        self.store.causality_region_curr_val().await
    }

    fn shard(&self) -> &str {
        self.store.shard()
    }

    async fn health(&self) -> Result<schema::SubgraphHealth, StoreError> {
        self.store.health().await
    }

    async fn flush(&self) -> Result<(), StoreError> {
        self.writer.flush().await
    }

    async fn restart(self: Arc<Self>) -> Result<Option<Arc<dyn WritableStoreTrait>>, StoreError> {
        if self.poisoned() {
            // When the writer is poisoned, the background thread has
            // finished since `start_writer` returns whenever it encounters
            // an error. Just to make extra-sure, we log a warning if the
            // join handle indicates that the writer hasn't stopped yet.
            let logger = self.store.logger.clone();
            match &self.writer {
                Writer::Sync(_) => { /* can't happen, a sync writer never gets poisoned */ }
                Writer::Async { join_handle, queue } => {
                    let err = match queue.check_err() {
                        Ok(()) => "error missing".to_string(),
                        Err(e) => e.to_string(),
                    };
                    if !join_handle.is_finished() {
                        warn!(logger, "Writer was poisoned, but background thread didn't finish. Creating new writer regardless"; "error" => err);
                    }
                }
            }
            let store = Arc::new(self.store.store.0.clone());
            let manifest_idx_and_name = self.store.manifest_idx_and_name.cheap_clone();
            store
                .writable(logger, self.store.site.id.into(), manifest_idx_and_name)
                .await
                .map(|store| Some(store))
        } else {
            Ok(None)
        }
    }
}
