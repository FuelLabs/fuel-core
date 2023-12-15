use anyhow::Error;
use futures03::{stream::Stream, Future, FutureExt};
use std::cmp;
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use super::block_stream::{
    BlockStream, BlockStreamEvent, BlockWithTriggers, ChainHeadUpdateStream, FirehoseCursor,
    TriggersAdapter, BUFFERED_BLOCK_STREAM_SIZE,
};
use super::{Block, BlockPtr, Blockchain};

use crate::components::store::BlockNumber;
use crate::data::subgraph::UnifiedMappingApiVersion;
use crate::prelude::*;

// A high number here forces a slow start.
const STARTING_PREVIOUS_TRIGGERS_PER_BLOCK: f64 = 1_000_000.0;

enum BlockStreamState<C>
where
    C: Blockchain,
{
    /// Starting or restarting reconciliation.
    ///
    /// Valid next states: Reconciliation
    BeginReconciliation,

    /// The BlockStream is reconciling the subgraph store state with the chain store state.
    ///
    /// Valid next states: YieldingBlocks, Idle, BeginReconciliation (in case of revert)
    Reconciliation(Pin<Box<dyn Future<Output = Result<NextBlocks<C>, Error>> + Send>>),

    /// The BlockStream is emitting blocks that must be processed in order to bring the subgraph
    /// store up to date with the chain store.
    ///
    /// Valid next states: BeginReconciliation
    YieldingBlocks(Box<VecDeque<BlockWithTriggers<C>>>),

    /// The BlockStream experienced an error and is pausing before attempting to produce
    /// blocks again.
    ///
    /// Valid next states: BeginReconciliation
    RetryAfterDelay(Pin<Box<dyn Future<Output = Result<(), Error>> + Send>>),

    /// The BlockStream has reconciled the subgraph store and chain store states.
    /// No more work is needed until a chain head update.
    ///
    /// Valid next states: BeginReconciliation
    Idle,
}

/// A single next step to take in reconciling the state of the subgraph store with the state of the
/// chain store.
enum ReconciliationStep<C>
where
    C: Blockchain,
{
    /// Revert(to) the block the subgraph should be reverted to, so it becomes the new subgraph
    /// head.
    Revert(BlockPtr),

    /// Move forwards, processing one or more blocks. Second element is the block range size.
    ProcessDescendantBlocks(Vec<BlockWithTriggers<C>>, BlockNumber),

    /// This step is a no-op, but we need to check again for a next step.
    Retry,

    /// Subgraph pointer now matches chain head pointer.
    /// Reconciliation is complete.
    Done,
}

struct PollingBlockStreamContext<C>
where
    C: Blockchain,
{
    chain_store: Arc<dyn ChainStore>,
    adapter: Arc<dyn TriggersAdapter<C>>,
    node_id: NodeId,
    subgraph_id: DeploymentHash,
    // This is not really a block number, but the (unsigned) difference
    // between two block numbers
    reorg_threshold: BlockNumber,
    filter: Arc<C::TriggerFilter>,
    start_blocks: Vec<BlockNumber>,
    logger: Logger,
    previous_triggers_per_block: f64,
    // Not a BlockNumber, but the difference between two block numbers
    previous_block_range_size: BlockNumber,
    // Not a BlockNumber, but the difference between two block numbers
    max_block_range_size: BlockNumber,
    target_triggers_per_block_range: u64,
    unified_api_version: UnifiedMappingApiVersion,
    current_block: Option<BlockPtr>,
}

impl<C: Blockchain> Clone for PollingBlockStreamContext<C> {
    fn clone(&self) -> Self {
        Self {
            chain_store: self.chain_store.cheap_clone(),
            adapter: self.adapter.clone(),
            node_id: self.node_id.clone(),
            subgraph_id: self.subgraph_id.clone(),
            reorg_threshold: self.reorg_threshold,
            filter: self.filter.clone(),
            start_blocks: self.start_blocks.clone(),
            logger: self.logger.clone(),
            previous_triggers_per_block: self.previous_triggers_per_block,
            previous_block_range_size: self.previous_block_range_size,
            max_block_range_size: self.max_block_range_size,
            target_triggers_per_block_range: self.target_triggers_per_block_range,
            unified_api_version: self.unified_api_version.clone(),
            current_block: self.current_block.clone(),
        }
    }
}

pub struct PollingBlockStream<C: Blockchain> {
    state: BlockStreamState<C>,
    consecutive_err_count: u32,
    chain_head_update_stream: ChainHeadUpdateStream,
    ctx: PollingBlockStreamContext<C>,
}

// This is the same as `ReconciliationStep` but without retries.
enum NextBlocks<C>
where
    C: Blockchain,
{
    /// Blocks and range size
    Blocks(VecDeque<BlockWithTriggers<C>>, BlockNumber),

    // The payload is block the subgraph should be reverted to, so it becomes the new subgraph head.
    Revert(BlockPtr),
    Done,
}

impl<C> PollingBlockStream<C>
where
    C: Blockchain,
{
    pub fn new(
        chain_store: Arc<dyn ChainStore>,
        chain_head_update_stream: ChainHeadUpdateStream,
        adapter: Arc<dyn TriggersAdapter<C>>,
        node_id: NodeId,
        subgraph_id: DeploymentHash,
        filter: Arc<C::TriggerFilter>,
        start_blocks: Vec<BlockNumber>,
        reorg_threshold: BlockNumber,
        logger: Logger,
        max_block_range_size: BlockNumber,
        target_triggers_per_block_range: u64,
        unified_api_version: UnifiedMappingApiVersion,
        start_block: Option<BlockPtr>,
    ) -> Self {
        Self {
            state: BlockStreamState::BeginReconciliation,
            consecutive_err_count: 0,
            chain_head_update_stream,
            ctx: PollingBlockStreamContext {
                current_block: start_block,
                chain_store,
                adapter,
                node_id,
                subgraph_id,
                reorg_threshold,
                logger,
                filter,
                start_blocks,
                previous_triggers_per_block: STARTING_PREVIOUS_TRIGGERS_PER_BLOCK,
                previous_block_range_size: 1,
                max_block_range_size,
                target_triggers_per_block_range,
                unified_api_version,
            },
        }
    }
}

impl<C> PollingBlockStreamContext<C>
where
    C: Blockchain,
{
    /// Perform reconciliation steps until there are blocks to yield or we are up-to-date.
    async fn next_blocks(&self) -> Result<NextBlocks<C>, Error> {
        let ctx = self.clone();

        loop {
            match ctx.get_next_step().await? {
                ReconciliationStep::ProcessDescendantBlocks(next_blocks, range_size) => {
                    return Ok(NextBlocks::Blocks(
                        next_blocks.into_iter().collect(),
                        range_size,
                    ));
                }
                ReconciliationStep::Retry => {
                    continue;
                }
                ReconciliationStep::Done => {
                    return Ok(NextBlocks::Done);
                }
                ReconciliationStep::Revert(parent_ptr) => {
                    return Ok(NextBlocks::Revert(parent_ptr))
                }
            }
        }
    }

    /// Determine the next reconciliation step. Does not modify Store or ChainStore.
    async fn get_next_step(&self) -> Result<ReconciliationStep<C>, Error> {
        let ctx = self.clone();
        let start_blocks = self.start_blocks.clone();
        let max_block_range_size = self.max_block_range_size;

        // Get pointers from database for comparison
        let head_ptr_opt = ctx.chain_store.chain_head_ptr().await?;
        let subgraph_ptr = self.current_block.clone();

        // If chain head ptr is not set yet
        let head_ptr = match head_ptr_opt {
            Some(head_ptr) => head_ptr,

            // Don't do any reconciliation until the chain store has more blocks
            None => {
                return Ok(ReconciliationStep::Done);
            }
        };

        trace!(
            ctx.logger, "Chain head pointer";
            "hash" => format!("{:?}", head_ptr.hash),
            "number" => &head_ptr.number
        );
        trace!(
            ctx.logger, "Subgraph pointer";
            "hash" => format!("{:?}", subgraph_ptr.as_ref().map(|block| &block.hash)),
            "number" => subgraph_ptr.as_ref().map(|block| &block.number),
        );

        // Make sure not to include genesis in the reorg threshold.
        let reorg_threshold = ctx.reorg_threshold.min(head_ptr.number);

        // Only continue if the subgraph block ptr is behind the head block ptr.
        // subgraph_ptr > head_ptr shouldn't happen, but if it does, it's safest to just stop.
        if let Some(ptr) = &subgraph_ptr {
            if ptr.number >= head_ptr.number {
                return Ok(ReconciliationStep::Done);
            }
        }

        // Subgraph ptr is behind head ptr.
        // Let's try to move the subgraph ptr one step in the right direction.
        // First question: which direction should the ptr be moved?
        //
        // We will use a different approach to deciding the step direction depending on how far
        // the subgraph ptr is behind the head ptr.
        //
        // Normally, we need to worry about chain reorganizations -- situations where the
        // Ethereum client discovers a new longer chain of blocks different from the one we had
        // processed so far, forcing us to rollback one or more blocks we had already
        // processed.
        // We can't assume that blocks we receive are permanent.
        //
        // However, as a block receives more and more confirmations, eventually it becomes safe
        // to assume that that block will be permanent.
        // The probability of a block being "uncled" approaches zero as more and more blocks
        // are chained on after that block.
        // Eventually, the probability is so low, that a block is effectively permanent.
        // The "effectively permanent" part is what makes blockchains useful.
        // See here for more discussion:
        // https://blog.ethereum.org/2016/05/09/on-settlement-finality/
        //
        // Accordingly, if the subgraph ptr is really far behind the head ptr, then we can
        // trust that the Ethereum node knows what the real, permanent block is for that block
        // number.
        // We'll define "really far" to mean "greater than reorg_threshold blocks".
        //
        // If the subgraph ptr is not too far behind the head ptr (i.e. less than
        // reorg_threshold blocks behind), then we have to allow for the possibility that the
        // block might be on the main chain now, but might become uncled in the future.
        //
        // Most importantly: Our ability to make this assumption (or not) will determine what
        // Ethereum RPC calls can give us accurate data without race conditions.
        // (This is mostly due to some unfortunate API design decisions on the Ethereum side)
        if subgraph_ptr.is_none()
            || (head_ptr.number - subgraph_ptr.as_ref().unwrap().number) > reorg_threshold
        {
            // Since we are beyond the reorg threshold, the Ethereum node knows what block has
            // been permanently assigned this block number.
            // This allows us to ask the node: does subgraph_ptr point to a block that was
            // permanently accepted into the main chain, or does it point to a block that was
            // uncled?
            let is_on_main_chain = match &subgraph_ptr {
                Some(ptr) => ctx.adapter.is_on_main_chain(ptr.clone()).await?,
                None => true,
            };
            if !is_on_main_chain {
                // The subgraph ptr points to a block that was uncled.
                // We need to revert this block.
                //
                // Note: We can safely unwrap the subgraph ptr here, because
                // if it was `None`, `is_on_main_chain` would be true.
                let from = subgraph_ptr.unwrap();
                let parent = self.parent_ptr(&from, "is_on_main_chain").await?;

                return Ok(ReconciliationStep::Revert(parent));
            }

            // The subgraph ptr points to a block on the main chain.
            // This means that the last block we processed does not need to be
            // reverted.
            // Therefore, our direction of travel will be forward, towards the
            // chain head.

            // As an optimization, instead of advancing one block, we will use an
            // Ethereum RPC call to find the first few blocks that have event(s) we
            // are interested in that lie within the block range between the subgraph ptr
            // and either the next data source start_block or the reorg threshold.
            // Note that we use block numbers here.
            // This is an artifact of Ethereum RPC limitations.
            // It is only safe to use block numbers because we are beyond the reorg
            // threshold.

            // Start with first block after subgraph ptr; if the ptr is None,
            // then we start with the genesis block
            let from = subgraph_ptr.map_or(0, |ptr| ptr.number + 1);

            // Get the next subsequent data source start block to ensure the block
            // range is aligned with data source. This is not necessary for
            // correctness, but it avoids an ineffecient situation such as the range
            // being 0..100 and the start block for a data source being 99, then
            // `calls_in_block_range` would request unecessary traces for the blocks
            // 0 to 98 because the start block is within the range.
            let next_start_block: BlockNumber = start_blocks
                .into_iter()
                .filter(|block_num| block_num > &from)
                .min()
                .unwrap_or(BLOCK_NUMBER_MAX);

            // End either just before the the next data source start_block or just
            // prior to the reorg threshold. It isn't safe to go farther than the
            // reorg threshold due to race conditions.
            let to_limit = cmp::min(head_ptr.number - reorg_threshold, next_start_block - 1);

            // Calculate the range size according to the target number of triggers,
            // respecting the global maximum and also not increasing too
            // drastically from the previous block range size.
            //
            // An example of the block range dynamics:
            // - Start with a block range of 1, target of 1000.
            // - Scan 1 block:
            //   0 triggers found, max_range_size = 10, range_size = 10
            // - Scan 10 blocks:
            //   2 triggers found, 0.2 per block, range_size = 1000 / 0.2 = 5000
            // - Scan 5000 blocks:
            //   10000 triggers found, 2 per block, range_size = 1000 / 2 = 500
            // - Scan 500 blocks:
            //   1000 triggers found, 2 per block, range_size = 1000 / 2 = 500
            let range_size_upper_limit =
                max_block_range_size.min(ctx.previous_block_range_size * 10);
            let range_size = if ctx.previous_triggers_per_block == 0.0 {
                range_size_upper_limit
            } else {
                (self.target_triggers_per_block_range as f64 / ctx.previous_triggers_per_block)
                    .max(1.0)
                    .min(range_size_upper_limit as f64) as BlockNumber
            };
            let to = cmp::min(from + range_size - 1, to_limit);

            info!(
                ctx.logger,
                "Scanning blocks [{}, {}]", from, to;
                "range_size" => range_size
            );

            let blocks = self.adapter.scan_triggers(from, to, &self.filter).await?;

            Ok(ReconciliationStep::ProcessDescendantBlocks(
                blocks, range_size,
            ))
        } else {
            // The subgraph ptr is not too far behind the head ptr.
            // This means a few things.
            //
            // First, because we are still within the reorg threshold,
            // we can't trust the Ethereum RPC methods that use block numbers.
            // Block numbers in this region are not yet immutable pointers to blocks;
            // the block associated with a particular block number on the Ethereum node could
            // change under our feet at any time.
            //
            // Second, due to how the BlockIngestor is designed, we get a helpful guarantee:
            // the head block and at least its reorg_threshold most recent ancestors will be
            // present in the block store.
            // This allows us to work locally in the block store instead of relying on
            // Ethereum RPC calls, so that we are not subject to the limitations of the RPC
            // API.

            // To determine the step direction, we need to find out if the subgraph ptr refers
            // to a block that is an ancestor of the head block.
            // We can do so by walking back up the chain from the head block to the appropriate
            // block number, and checking to see if the block we found matches the
            // subgraph_ptr.

            let subgraph_ptr =
                subgraph_ptr.expect("subgraph block pointer should not be `None` here");

            // Precondition: subgraph_ptr.number < head_ptr.number
            // Walk back to one block short of subgraph_ptr.number
            let offset = head_ptr.number - subgraph_ptr.number - 1;

            // In principle this block should be in the store, but we have seen this error for deep
            // reorgs in ropsten.
            let head_ancestor_opt = self.adapter.ancestor_block(head_ptr, offset).await?;

            match head_ancestor_opt {
                None => {
                    // Block is missing in the block store.
                    // This generally won't happen often, but can happen if the head ptr has
                    // been updated since we retrieved the head ptr, and the block store has
                    // been garbage collected.
                    // It's easiest to start over at this point.
                    Ok(ReconciliationStep::Retry)
                }
                Some(head_ancestor) => {
                    // We stopped one block short, so we'll compare the parent hash to the
                    // subgraph ptr.
                    if head_ancestor.parent_hash().as_ref() == Some(&subgraph_ptr.hash) {
                        // The subgraph ptr is an ancestor of the head block.
                        // We cannot use an RPC call here to find the first interesting block
                        // due to the race conditions previously mentioned,
                        // so instead we will advance the subgraph ptr by one block.
                        // Note that head_ancestor is a child of subgraph_ptr.
                        let block = self
                            .adapter
                            .triggers_in_block(&self.logger, head_ancestor, &self.filter)
                            .await?;
                        Ok(ReconciliationStep::ProcessDescendantBlocks(vec![block], 1))
                    } else {
                        let parent = self.parent_ptr(&subgraph_ptr, "nonfinal").await?;

                        // The subgraph ptr is not on the main chain.
                        // We will need to step back (possibly repeatedly) one block at a time
                        // until we are back on the main chain.
                        Ok(ReconciliationStep::Revert(parent))
                    }
                }
            }
        }
    }

    async fn parent_ptr(&self, block_ptr: &BlockPtr, reason: &str) -> Result<BlockPtr, Error> {
        let ptr =
            self.adapter.parent_ptr(block_ptr).await?.ok_or_else(|| {
                anyhow!("Failed to get parent pointer for {block_ptr} ({reason})")
            })?;

        Ok(ptr)
    }
}

impl<C: Blockchain> BlockStream<C> for PollingBlockStream<C> {
    fn buffer_size_hint(&self) -> usize {
        BUFFERED_BLOCK_STREAM_SIZE
    }
}

impl<C: Blockchain> Stream for PollingBlockStream<C> {
    type Item = Result<BlockStreamEvent<C>, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let result = loop {
            match &mut self.state {
                BlockStreamState::BeginReconciliation => {
                    // Start the reconciliation process by asking for blocks
                    let ctx = self.ctx.clone();
                    let fut = async move { ctx.next_blocks().await };
                    self.state = BlockStreamState::Reconciliation(fut.boxed());
                }

                // Waiting for the reconciliation to complete or yield blocks
                BlockStreamState::Reconciliation(next_blocks_future) => {
                    match next_blocks_future.poll_unpin(cx) {
                        Poll::Ready(Ok(next_block_step)) => match next_block_step {
                            NextBlocks::Blocks(next_blocks, block_range_size) => {
                                // We had only one error, so we infer that reducing the range size is
                                // what fixed it. Reduce the max range size to prevent future errors.
                                // See: 018c6df4-132f-4acc-8697-a2d64e83a9f0
                                if self.consecutive_err_count == 1 {
                                    // Reduce the max range size by 10%, but to no less than 10.
                                    self.ctx.max_block_range_size =
                                        (self.ctx.max_block_range_size * 9 / 10).max(10);
                                }
                                self.consecutive_err_count = 0;

                                let total_triggers =
                                    next_blocks.iter().map(|b| b.trigger_count()).sum::<usize>();
                                self.ctx.previous_triggers_per_block =
                                    total_triggers as f64 / block_range_size as f64;
                                self.ctx.previous_block_range_size = block_range_size;
                                if total_triggers > 0 {
                                    debug!(
                                        self.ctx.logger,
                                        "Processing {} triggers", total_triggers
                                    );
                                }

                                // Switch to yielding state until next_blocks is depleted
                                self.state =
                                    BlockStreamState::YieldingBlocks(Box::new(next_blocks));

                                // Yield the first block in next_blocks
                                continue;
                            }
                            // Reconciliation completed. We're caught up to chain head.
                            NextBlocks::Done => {
                                // Reset error count
                                self.consecutive_err_count = 0;

                                // Switch to idle
                                self.state = BlockStreamState::Idle;

                                // Poll for chain head update
                                continue;
                            }
                            NextBlocks::Revert(parent_ptr) => {
                                self.ctx.current_block = Some(parent_ptr.clone());

                                self.state = BlockStreamState::BeginReconciliation;
                                break Poll::Ready(Some(Ok(BlockStreamEvent::Revert(
                                    parent_ptr,
                                    FirehoseCursor::None,
                                ))));
                            }
                        },
                        Poll::Pending => break Poll::Pending,
                        Poll::Ready(Err(e)) => {
                            // Reset the block range size in an attempt to recover from the error.
                            // See also: 018c6df4-132f-4acc-8697-a2d64e83a9f0
                            self.ctx.previous_triggers_per_block =
                                STARTING_PREVIOUS_TRIGGERS_PER_BLOCK;
                            self.consecutive_err_count += 1;

                            // Pause before trying again
                            let secs = (5 * self.consecutive_err_count).max(120) as u64;

                            self.state = BlockStreamState::RetryAfterDelay(Box::pin(
                                tokio::time::sleep(Duration::from_secs(secs)).map(Ok),
                            ));

                            break Poll::Ready(Some(Err(e)));
                        }
                    }
                }

                // Yielding blocks from reconciliation process
                BlockStreamState::YieldingBlocks(ref mut next_blocks) => {
                    match next_blocks.pop_front() {
                        // Yield one block
                        Some(next_block) => {
                            self.ctx.current_block = Some(next_block.block.ptr());

                            break Poll::Ready(Some(Ok(BlockStreamEvent::ProcessBlock(
                                next_block,
                                FirehoseCursor::None,
                            ))));
                        }

                        // Done yielding blocks
                        None => {
                            self.state = BlockStreamState::BeginReconciliation;
                        }
                    }
                }

                // Pausing after an error, before looking for more blocks
                BlockStreamState::RetryAfterDelay(ref mut delay) => match delay.as_mut().poll(cx) {
                    Poll::Ready(Ok(..)) | Poll::Ready(Err(_)) => {
                        self.state = BlockStreamState::BeginReconciliation;
                    }

                    Poll::Pending => {
                        break Poll::Pending;
                    }
                },

                // Waiting for a chain head update
                BlockStreamState::Idle => {
                    match Pin::new(self.chain_head_update_stream.as_mut()).poll_next(cx) {
                        // Chain head was updated
                        Poll::Ready(Some(())) => {
                            self.state = BlockStreamState::BeginReconciliation;
                        }

                        // Chain head update stream ended
                        Poll::Ready(None) => {
                            // Should not happen
                            return Poll::Ready(Some(Err(anyhow::anyhow!(
                                "chain head update stream ended unexpectedly"
                            ))));
                        }

                        Poll::Pending => break Poll::Pending,
                    }
                }
            }
        };

        result
    }
}
