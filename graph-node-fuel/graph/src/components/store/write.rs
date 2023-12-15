//! Data structures and helpers for writing subgraph changes to the store
use std::collections::HashSet;

use crate::{
    blockchain::{block_stream::FirehoseCursor, BlockPtr},
    cheap_clone::CheapClone,
    components::subgraph::Entity,
    constraint_violation,
    data::{store::Id, subgraph::schema::SubgraphError},
    data_source::CausalityRegion,
    prelude::DeploymentHash,
    util::cache_weight::CacheWeight,
};

use super::{BlockNumber, EntityKey, EntityType, StoreError, StoreEvent, StoredDynamicDataSource};

/// A data structure similar to `EntityModification`, but tagged with a
/// block. We might eventually replace `EntityModification` with this, but
/// until the dust settles, we'll keep them separate.
///
/// This is geared towards how we persist entity changes: there are only
/// ever two operations we perform on them, clamping the range of an
/// existing entity version, and writing a new entity version.
///
/// The difference between `Insert` and `Overwrite` is that `Overwrite`
/// requires that we clamp an existing prior version of the entity at
/// `block`. We only ever get an `Overwrite` if such a version actually
/// exists. `Insert` simply inserts a new row into the underlying table,
/// assuming that there is no need to fix up any prior version.
///
/// The `end` field for `Insert` and `Overwrite` indicates whether the
/// entity exists now: if it is `None`, the entity currently exists, but if
/// it is `Some(_)`, it was deleted, for example, by folding a `Remove` or
/// `Overwrite` into this operation. The entity version will only be visible
/// before `end`, excluding `end`. This folding, which happens in
/// `append_row`, eliminates an update in the database which would otherwise
/// be needed to clamp the open block range of the entity to the block
/// contained in `end`
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntityModification {
    /// Insert the entity
    Insert {
        key: EntityKey,
        data: Entity,
        block: BlockNumber,
        end: Option<BlockNumber>,
    },
    /// Update the entity by overwriting it
    Overwrite {
        key: EntityKey,
        data: Entity,
        block: BlockNumber,
        end: Option<BlockNumber>,
    },
    /// Remove the entity
    Remove { key: EntityKey, block: BlockNumber },
}

/// A helper struct for passing entity writes to the outside world, viz. the
/// SQL query generation that inserts rows
pub struct EntityWrite<'a> {
    pub id: &'a Id,
    pub entity: &'a Entity,
    pub causality_region: CausalityRegion,
    pub block: BlockNumber,
    // The end of the block range for which this write is valid. The value
    // of `end` itself is not included in the range
    pub end: Option<BlockNumber>,
}

impl<'a> TryFrom<&'a EntityModification> for EntityWrite<'a> {
    type Error = ();

    fn try_from(emod: &'a EntityModification) -> Result<Self, Self::Error> {
        match emod {
            EntityModification::Insert {
                key,
                data,
                block,
                end,
            }
            | EntityModification::Overwrite {
                key,
                data,
                block,
                end,
            } => Ok(EntityWrite {
                id: &key.entity_id,
                entity: data,
                causality_region: key.causality_region,
                block: *block,
                end: *end,
            }),

            EntityModification::Remove { .. } => Err(()),
        }
    }
}

impl EntityModification {
    pub fn id(&self) -> &Id {
        match self {
            EntityModification::Insert { key, .. }
            | EntityModification::Overwrite { key, .. }
            | EntityModification::Remove { key, .. } => &key.entity_id,
        }
    }

    fn block(&self) -> BlockNumber {
        match self {
            EntityModification::Insert { block, .. }
            | EntityModification::Overwrite { block, .. }
            | EntityModification::Remove { block, .. } => *block,
        }
    }

    /// Return `true` if `self` requires a write operation, i.e.,insert of a
    /// new row, for either a new or an existing entity
    fn is_write(&self) -> bool {
        match self {
            EntityModification::Insert { .. } | EntityModification::Overwrite { .. } => true,
            EntityModification::Remove { .. } => false,
        }
    }

    /// Return the details of the write if `self` is a write operation for a
    /// new or an existing entity
    fn as_write(&self) -> Option<EntityWrite> {
        EntityWrite::try_from(self).ok()
    }

    /// Return `true` if `self` requires clamping of an existing version
    fn is_clamp(&self) -> bool {
        match self {
            EntityModification::Insert { .. } => false,
            EntityModification::Overwrite { .. } | EntityModification::Remove { .. } => true,
        }
    }

    pub fn creates_entity(&self) -> bool {
        match self {
            EntityModification::Insert { .. } => true,
            EntityModification::Overwrite { .. } | EntityModification::Remove { .. } => false,
        }
    }

    fn entity_count_change(&self) -> i32 {
        match self {
            EntityModification::Insert { end: None, .. } => 1,
            EntityModification::Insert { end: Some(_), .. } => {
                // Insert followed by a remove
                0
            }
            EntityModification::Overwrite { end: None, .. } => 0,
            EntityModification::Overwrite { end: Some(_), .. } => {
                // Overwrite followed by a remove
                -1
            }
            EntityModification::Remove { .. } => -1,
        }
    }

    fn clamp(&mut self, block: BlockNumber) -> Result<(), StoreError> {
        use EntityModification::*;

        match self {
            Insert { end, .. } | Overwrite { end, .. } => {
                if end.is_some() {
                    return Err(constraint_violation!(
                        "can not clamp {:?} to block {}",
                        self,
                        block
                    ));
                }
                *end = Some(block);
            }
            Remove { .. } => {
                return Err(constraint_violation!(
                    "can not clamp block range for removal of {:?} to {}",
                    self,
                    block
                ))
            }
        }
        Ok(())
    }

    /// Turn an `Overwrite` into an `Insert`, return an error if this is a `Remove`
    fn as_insert(self, entity_type: &EntityType) -> Result<Self, StoreError> {
        use EntityModification::*;

        match self {
            Insert { .. } => Ok(self),
            Overwrite {
                key,
                data,
                block,
                end,
            } => Ok(Insert {
                key,
                data,
                block,
                end,
            }),
            Remove { key, .. } => {
                return Err(constraint_violation!(
                    "a remove for {}[{}] can not be converted into an insert",
                    entity_type,
                    key.entity_id
                ))
            }
        }
    }

    fn as_entity_op(&self, at: BlockNumber) -> EntityOp<'_> {
        debug_assert!(self.block() <= at);

        use EntityModification::*;

        match self {
            Insert {
                data,
                key,
                end: None,
                ..
            }
            | Overwrite {
                data,
                key,
                end: None,
                ..
            } => EntityOp::Write { key, entity: data },
            Insert {
                data,
                key,
                end: Some(end),
                ..
            }
            | Overwrite {
                data,
                key,
                end: Some(end),
                ..
            } if at < *end => EntityOp::Write { key, entity: data },
            Insert {
                key, end: Some(_), ..
            }
            | Overwrite {
                key, end: Some(_), ..
            }
            | Remove { key, .. } => EntityOp::Remove { key },
        }
    }
}

impl EntityModification {
    pub fn insert(key: EntityKey, data: Entity, block: BlockNumber) -> Self {
        EntityModification::Insert {
            key,
            data,
            block,
            end: None,
        }
    }

    pub fn overwrite(key: EntityKey, data: Entity, block: BlockNumber) -> Self {
        EntityModification::Overwrite {
            key,
            data,
            block,
            end: None,
        }
    }

    pub fn remove(key: EntityKey, block: BlockNumber) -> Self {
        EntityModification::Remove { key, block }
    }

    pub fn key(&self) -> &EntityKey {
        use EntityModification::*;
        match self {
            Insert { key, .. } | Overwrite { key, .. } | Remove { key, .. } => key,
        }
    }
}

/// A list of entity changes grouped by the entity type
#[derive(Debug)]
pub struct RowGroup {
    pub entity_type: EntityType,
    /// All changes for this entity type, ordered by block; i.e., if `i < j`
    /// then `rows[i].block() <= rows[j].block()`. Several methods on this
    /// struct rely on the fact that this ordering is observed.
    rows: Vec<EntityModification>,

    immutable: bool,
}

impl RowGroup {
    pub fn new(entity_type: EntityType, immutable: bool) -> Self {
        Self {
            entity_type,
            rows: Vec::new(),
            immutable,
        }
    }

    pub fn push(&mut self, emod: EntityModification, block: BlockNumber) -> Result<(), StoreError> {
        let is_forward = self
            .rows
            .last()
            .map(|emod| emod.block() <= block)
            .unwrap_or(true);
        if !is_forward {
            // unwrap: we only get here when `last()` is `Some`
            let last_block = self.rows.last().map(|emod| emod.block()).unwrap();
            return Err(constraint_violation!(
                "we already have a modification for block {}, can not append {:?}",
                last_block,
                emod
            ));
        }

        self.append_row(emod)
    }

    fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Return the change in entity count that will result from applying
    /// writing this row group to the database
    pub fn entity_count_change(&self) -> i32 {
        self.rows.iter().map(|row| row.entity_count_change()).sum()
    }

    /// Iterate over all changes that need clamping of the block range of an
    /// existing entity version
    pub fn clamps_by_block(&self) -> impl Iterator<Item = (BlockNumber, &[EntityModification])> {
        ClampsByBlockIterator::new(self)
    }

    /// Iterate over all changes that require writing a new entity version
    pub fn writes(&self) -> impl Iterator<Item = &EntityModification> {
        self.rows.iter().filter(|row| row.is_write())
    }

    /// Return an iterator over all writes in chunks. The returned
    /// `WriteChunker` is an iterator that produces `WriteChunk`s, which are
    /// the iterators over the writes. Each `WriteChunk` has `chunk_size`
    /// elements, except for the last one which might have fewer
    pub fn write_chunks<'a>(&'a self, chunk_size: usize) -> WriteChunker<'a> {
        WriteChunker::new(self, chunk_size)
    }

    pub fn has_clamps(&self) -> bool {
        self.rows.iter().any(|row| row.is_clamp())
    }

    pub fn last_op(&self, key: &EntityKey, at: BlockNumber) -> Option<EntityOp<'_>> {
        self.rows
            .iter()
            // We are scanning backwards, i.e., in descendng order of
            // `emod.block()`. Therefore, the first `emod` we encounter
            // whose block is before `at` is the one in effect
            .rfind(|emod| emod.key() == key && emod.block() <= at)
            .map(|emod| emod.as_entity_op(at))
    }

    pub fn effective_ops(&self, at: BlockNumber) -> impl Iterator<Item = EntityOp<'_>> {
        let mut seen = HashSet::new();
        self.rows
            .iter()
            .rev()
            .filter(move |emod| {
                if emod.block() <= at {
                    seen.insert(emod.id())
                } else {
                    false
                }
            })
            .map(move |emod| emod.as_entity_op(at))
    }

    /// Find the most recent entry for `id`
    fn prev_row_mut(&mut self, id: &Id) -> Option<&mut EntityModification> {
        self.rows.iter_mut().rfind(|emod| emod.id() == id)
    }

    /// Append `row` to `self.rows` by combining it with a previously
    /// existing row, if that is possible
    fn append_row(&mut self, row: EntityModification) -> Result<(), StoreError> {
        if self.immutable {
            match row {
                EntityModification::Insert { .. } => {
                    self.rows.push(row);
                }
                EntityModification::Overwrite { .. } | EntityModification::Remove { .. } => {
                    return Err(constraint_violation!(
                        "immutable entity type {} only allows inserts, not {:?}",
                        self.entity_type,
                        row
                    ));
                }
            }
            return Ok(());
        }

        if let Some(prev_row) = self.prev_row_mut(row.id()) {
            use EntityModification::*;

            if row.block() <= prev_row.block() {
                return Err(constraint_violation!(
                    "can not append operations that go backwards from {:?} to {:?}",
                    prev_row,
                    row
                ));
            }

            // The heart of the matter: depending on what `row` is, clamp
            // `prev_row` and either ignore `row` since it is not needed, or
            // turn it into an `Insert`, which also does not require
            // clamping an old version
            match (&*prev_row, &row) {
                (Insert { end: None, .. } | Overwrite { end: None, .. }, Insert { .. })
                | (Remove { .. }, Overwrite { .. } | Remove { .. })
                | (
                    Insert { end: Some(_), .. } | Overwrite { end: Some(_), .. },
                    Overwrite { .. } | Remove { .. },
                ) => {
                    return Err(constraint_violation!(
                        "impossible combination of entity operations: {:?} and then {:?}",
                        prev_row,
                        row
                    ))
                }
                (
                    Insert { end: Some(_), .. } | Overwrite { end: Some(_), .. } | Remove { .. },
                    Insert { .. },
                ) => {
                    // prev_row was deleted
                    self.rows.push(row);
                }
                (
                    Insert { end: None, .. } | Overwrite { end: None, .. },
                    Overwrite { block, .. },
                ) => {
                    prev_row.clamp(*block)?;
                    self.rows.push(row.as_insert(&self.entity_type)?);
                }
                (Insert { end: None, .. } | Overwrite { end: None, .. }, Remove { block, .. }) => {
                    prev_row.clamp(*block)?;
                }
            }
        } else {
            self.rows.push(row);
        }
        Ok(())
    }

    fn append(&mut self, group: RowGroup) -> Result<(), StoreError> {
        if self.entity_type != group.entity_type {
            return Err(constraint_violation!(
                "Can not append a row group for {} to a row group for {}",
                group.entity_type,
                self.entity_type
            ));
        }

        self.rows.reserve(group.rows.len());
        for row in group.rows {
            self.append_row(row)?;
        }

        Ok(())
    }

    pub fn ids(&self) -> impl Iterator<Item = &Id> {
        self.rows.iter().map(|emod| emod.id())
    }
}

struct ClampsByBlockIterator<'a> {
    position: usize,
    rows: &'a [EntityModification],
}

impl<'a> ClampsByBlockIterator<'a> {
    fn new(group: &'a RowGroup) -> Self {
        ClampsByBlockIterator {
            position: 0,
            rows: &group.rows,
        }
    }
}

impl<'a> Iterator for ClampsByBlockIterator<'a> {
    type Item = (BlockNumber, &'a [EntityModification]);

    fn next(&mut self) -> Option<Self::Item> {
        // Make sure we start on a clamp
        while self.position < self.rows.len() && !self.rows[self.position].is_clamp() {
            self.position += 1;
        }
        if self.position >= self.rows.len() {
            return None;
        }
        let block = self.rows[self.position].block();
        let mut next = self.position;
        // Collect consecutive clamps
        while next < self.rows.len()
            && self.rows[next].block() == block
            && self.rows[next].is_clamp()
        {
            next += 1;
        }
        let res = Some((block, &self.rows[self.position..next]));
        self.position = next;
        res
    }
}

/// A list of entity changes with one group per entity type
#[derive(Debug)]
pub struct RowGroups {
    pub groups: Vec<RowGroup>,
}

impl RowGroups {
    fn new() -> Self {
        Self { groups: Vec::new() }
    }

    fn group(&self, entity_type: &EntityType) -> Option<&RowGroup> {
        self.groups
            .iter()
            .find(|group| &group.entity_type == entity_type)
    }

    /// Return a mutable reference to an existing group, or create a new one
    /// if there isn't one yet and return a reference to that
    fn group_entry(&mut self, entity_type: &EntityType) -> &mut RowGroup {
        let pos = self
            .groups
            .iter()
            .position(|group| &group.entity_type == entity_type);
        match pos {
            Some(pos) => &mut self.groups[pos],
            None => {
                let immutable = entity_type.is_immutable();
                self.groups
                    .push(RowGroup::new(entity_type.clone(), immutable));
                // unwrap: we just pushed an entry
                self.groups.last_mut().unwrap()
            }
        }
    }

    fn entity_count(&self) -> usize {
        self.groups.iter().map(|group| group.row_count()).sum()
    }

    fn append(&mut self, other: RowGroups) -> Result<(), StoreError> {
        for group in other.groups {
            self.group_entry(&group.entity_type).append(group)?;
        }
        Ok(())
    }
}

/// Data sources data grouped by block
#[derive(Debug)]
pub struct DataSources {
    pub entries: Vec<(BlockPtr, Vec<StoredDynamicDataSource>)>,
}

impl DataSources {
    fn new(ptr: BlockPtr, entries: Vec<StoredDynamicDataSource>) -> Self {
        let entries = if entries.is_empty() {
            Vec::new()
        } else {
            vec![(ptr, entries)]
        };
        DataSources { entries }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.iter().all(|(_, dss)| dss.is_empty())
    }

    fn append(&mut self, mut other: DataSources) {
        self.entries.append(&mut other.entries);
    }
}

/// Indicate to code that looks up entities from the in-memory batch whether
/// the entity in question will be written or removed at the block of the
/// lookup
#[derive(Debug, PartialEq)]
pub enum EntityOp<'a> {
    /// There is a new version of the entity that will be written
    Write {
        key: &'a EntityKey,
        entity: &'a Entity,
    },
    /// The entity has been removed
    Remove { key: &'a EntityKey },
}

/// A write batch. This data structure encapsulates all the things that need
/// to be changed to persist the output of mappings up to a certain block.
#[derive(Debug)]
pub struct Batch {
    /// The last block for which this batch contains changes
    pub block_ptr: BlockPtr,
    /// The first block for which this batch contains changes
    pub first_block: BlockNumber,
    /// The firehose cursor corresponding to `block_ptr`
    pub firehose_cursor: FirehoseCursor,
    mods: RowGroups,
    /// New data sources
    pub data_sources: DataSources,
    pub deterministic_errors: Vec<SubgraphError>,
    pub offchain_to_remove: DataSources,
    pub error: Option<StoreError>,
    pub is_non_fatal_errors_active: bool,
}

impl Batch {
    pub fn new(
        block_ptr: BlockPtr,
        firehose_cursor: FirehoseCursor,
        mut raw_mods: Vec<EntityModification>,
        data_sources: Vec<StoredDynamicDataSource>,
        deterministic_errors: Vec<SubgraphError>,
        offchain_to_remove: Vec<StoredDynamicDataSource>,
        is_non_fatal_errors_active: bool,
    ) -> Result<Self, StoreError> {
        let block = block_ptr.number;

        // Sort the modifications such that writes and clamps are
        // consecutive. It's not needed for correctness but helps with some
        // of the iterations, especially when we iterate with
        // `clamps_by_block` so we get only one run for each block
        raw_mods.sort_unstable_by_key(|emod| match emod {
            EntityModification::Insert { .. } => 2,
            EntityModification::Overwrite { .. } => 1,
            EntityModification::Remove { .. } => 0,
        });

        let mut mods = RowGroups::new();

        for m in raw_mods {
            mods.group_entry(&m.key().entity_type).push(m, block)?;
        }

        let data_sources = DataSources::new(block_ptr.cheap_clone(), data_sources);
        let offchain_to_remove = DataSources::new(block_ptr.cheap_clone(), offchain_to_remove);
        let first_block = block_ptr.number;
        Ok(Self {
            block_ptr,
            first_block,
            firehose_cursor,
            mods,
            data_sources,
            deterministic_errors,
            offchain_to_remove,
            error: None,
            is_non_fatal_errors_active,
        })
    }

    fn append_inner(&mut self, mut batch: Batch) -> Result<(), StoreError> {
        if batch.block_ptr.number <= self.block_ptr.number {
            return Err(constraint_violation!("Batches must go forward. Can't append a batch with block pointer {} to one with block pointer {}", batch.block_ptr, self.block_ptr));
        }

        self.block_ptr = batch.block_ptr;
        self.firehose_cursor = batch.firehose_cursor;
        self.mods.append(batch.mods)?;
        self.data_sources.append(batch.data_sources);
        self.deterministic_errors
            .append(&mut batch.deterministic_errors);
        self.offchain_to_remove.append(batch.offchain_to_remove);
        Ok(())
    }

    /// Append `batch` to `self` so that writing `self` afterwards has the
    /// same effect as writing `self` first and then `batch` in separate
    /// transactions.
    ///
    /// When this method returns an `Err`, the batch is marked as not
    /// healthy by setting `self.error` to `Some(_)` and must not be written
    /// as it will be in an indeterminate state.
    pub fn append(&mut self, batch: Batch) -> Result<(), StoreError> {
        let res = self.append_inner(batch);
        if let Err(e) = &res {
            self.error = Some(e.clone());
        }
        res
    }

    pub fn entity_count(&self) -> usize {
        self.mods.entity_count()
    }

    /// Find out whether the latest operation for the entity with type
    /// `entity_type` and `id` is going to write that entity, i.e., insert
    /// or overwrite it, or if it is going to remove it. If no change will
    /// be made to the entity, return `None`
    pub fn last_op(&self, key: &EntityKey, block: BlockNumber) -> Option<EntityOp<'_>> {
        self.mods.group(&key.entity_type)?.last_op(key, block)
    }

    pub fn effective_ops(
        &self,
        entity_type: &EntityType,
        at: BlockNumber,
    ) -> impl Iterator<Item = EntityOp> {
        self.mods
            .group(entity_type)
            .map(|group| group.effective_ops(at))
            .into_iter()
            .flatten()
    }

    pub fn new_data_sources(
        &self,
        at: BlockNumber,
    ) -> impl Iterator<Item = &StoredDynamicDataSource> {
        self.data_sources
            .entries
            .iter()
            .filter(move |(ptr, _)| ptr.number <= at)
            .map(|(_, ds)| ds)
            .flatten()
            .filter(|ds| {
                !self
                    .offchain_to_remove
                    .entries
                    .iter()
                    .any(|(_, entries)| entries.contains(ds))
            })
    }

    /// Generate a store event for all the changes that this batch makes
    pub fn store_event(&self, deployment: &DeploymentHash) -> StoreEvent {
        let entity_types = HashSet::from_iter(
            self.mods
                .groups
                .iter()
                .map(|group| group.entity_type.clone()),
        );
        StoreEvent::from_types(deployment, entity_types)
    }

    pub fn groups<'a>(&'a self) -> impl Iterator<Item = &'a RowGroup> {
        self.mods.groups.iter()
    }
}

impl CacheWeight for Batch {
    fn indirect_weight(&self) -> usize {
        self.mods.indirect_weight()
    }
}

impl CacheWeight for RowGroups {
    fn indirect_weight(&self) -> usize {
        self.groups.indirect_weight()
    }
}

impl CacheWeight for RowGroup {
    fn indirect_weight(&self) -> usize {
        self.rows.indirect_weight()
    }
}

impl CacheWeight for EntityModification {
    fn indirect_weight(&self) -> usize {
        match self {
            EntityModification::Insert { key, data, .. }
            | EntityModification::Overwrite { key, data, .. } => {
                key.indirect_weight() + data.indirect_weight()
            }
            EntityModification::Remove { key, .. } => key.indirect_weight(),
        }
    }
}

pub struct WriteChunker<'a> {
    group: &'a RowGroup,
    chunk_size: usize,
    position: usize,
}

impl<'a> WriteChunker<'a> {
    fn new(group: &'a RowGroup, chunk_size: usize) -> Self {
        Self {
            group,
            chunk_size,
            position: 0,
        }
    }
}

impl<'a> Iterator for WriteChunker<'a> {
    type Item = WriteChunk<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        // Produce a chunk according to the current `self.position`
        let res = if self.position < self.group.rows.len() {
            Some(WriteChunk {
                group: self.group,
                chunk_size: self.chunk_size,
                position: self.position,
            })
        } else {
            None
        };

        // Advance `self.position` to the start of the next chunk
        let mut count = 0;
        while count < self.chunk_size && self.position < self.group.rows.len() {
            if self.group.rows[self.position].is_write() {
                count += 1;
            }
            self.position += 1;
        }

        res
    }
}

#[derive(Debug)]
pub struct WriteChunk<'a> {
    group: &'a RowGroup,
    chunk_size: usize,
    position: usize,
}

impl<'a> WriteChunk<'a> {
    pub fn is_empty(&'a self) -> bool {
        self.iter().next().is_none()
    }

    pub fn iter(&self) -> WriteChunkIter<'a> {
        WriteChunkIter {
            group: self.group,
            chunk_size: self.chunk_size,
            position: self.position,
            count: 0,
        }
    }
}

impl<'a> IntoIterator for &WriteChunk<'a> {
    type Item = EntityWrite<'a>;

    type IntoIter = WriteChunkIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        WriteChunkIter {
            group: self.group,
            chunk_size: self.chunk_size,
            position: self.position,
            count: 0,
        }
    }
}

pub struct WriteChunkIter<'a> {
    group: &'a RowGroup,
    chunk_size: usize,
    position: usize,
    count: usize,
}

impl<'a> Iterator for WriteChunkIter<'a> {
    type Item = EntityWrite<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.count < self.chunk_size && self.position < self.group.rows.len() {
            let insert = self.group.rows[self.position].as_write();
            self.position += 1;
            if insert.is_some() {
                self.count += 1;
                return insert;
            }
        }
        return None;
    }
}

#[cfg(test)]
mod test {
    use crate::{
        components::store::{
            write::EntityModification, write::EntityOp, BlockNumber, EntityType, StoreError,
        },
        data::{store::Id, value::Word},
        entity,
        prelude::DeploymentHash,
        schema::InputSchema,
    };
    use lazy_static::lazy_static;

    use super::RowGroup;

    #[track_caller]
    fn check_runs(values: &[usize], blocks: &[BlockNumber], exp: &[(BlockNumber, &[usize])]) {
        fn as_id(n: &usize) -> Id {
            Id::String(Word::from(n.to_string()))
        }

        assert_eq!(values.len(), blocks.len());

        let rows = values
            .iter()
            .zip(blocks.iter())
            .map(|(value, block)| EntityModification::Remove {
                key: ROW_GROUP_TYPE.key(Id::String(Word::from(value.to_string()))),
                block: *block,
            })
            .collect();
        let group = RowGroup {
            entity_type: ENTRY_TYPE.clone(),
            rows,
            immutable: false,
        };
        let act = group
            .clamps_by_block()
            .map(|(block, entries)| {
                (
                    block,
                    entries
                        .iter()
                        .map(|entry| entry.id().clone())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        let exp = Vec::from_iter(
            exp.into_iter()
                .map(|(block, values)| (*block, Vec::from_iter(values.iter().map(as_id)))),
        );
        assert_eq!(exp, act);
    }

    #[test]
    fn run_iterator() {
        type RunList<'a> = &'a [(i32, &'a [usize])];

        let exp: RunList<'_> = &[(1, &[10, 11, 12])];
        check_runs(&[10, 11, 12], &[1, 1, 1], exp);

        let exp: RunList<'_> = &[(1, &[10, 11, 12]), (2, &[20, 21])];
        check_runs(&[10, 11, 12, 20, 21], &[1, 1, 1, 2, 2], exp);

        let exp: RunList<'_> = &[(1, &[10]), (2, &[20]), (1, &[11])];
        check_runs(&[10, 20, 11], &[1, 2, 1], exp);
    }

    // A very fake schema that allows us to get the entity types we need
    const GQL: &str = r#"
      type Thing @entity { id: ID!, count: Int! }
      type RowGroup @entity { id: ID! }
      type Entry @entity { id: ID! }
    "#;
    lazy_static! {
        static ref DEPLOYMENT: DeploymentHash = DeploymentHash::new("batchAppend").unwrap();
        static ref SCHEMA: InputSchema = InputSchema::parse(GQL, DEPLOYMENT.clone()).unwrap();
        static ref THING_TYPE: EntityType = SCHEMA.entity_type("Thing").unwrap();
        static ref ROW_GROUP_TYPE: EntityType = SCHEMA.entity_type("RowGroup").unwrap();
        static ref ENTRY_TYPE: EntityType = SCHEMA.entity_type("Entry").unwrap();
    }

    /// Convenient notation for changes to a fixed entity
    #[derive(Clone, Debug)]
    enum Mod {
        Ins(BlockNumber),
        Ovw(BlockNumber),
        Rem(BlockNumber),
        // clamped insert
        InsC(BlockNumber, BlockNumber),
        // clamped overwrite
        OvwC(BlockNumber, BlockNumber),
    }

    impl From<&Mod> for EntityModification {
        fn from(value: &Mod) -> Self {
            use Mod::*;

            let value = value.clone();
            let key = THING_TYPE.parse_key("one").unwrap();
            match value {
                Ins(block) => EntityModification::Insert {
                    key,
                    data: entity! { SCHEMA => id: "one", count: block },
                    block,
                    end: None,
                },
                Ovw(block) => EntityModification::Overwrite {
                    key,
                    data: entity! { SCHEMA => id: "one", count: block },
                    block,
                    end: None,
                },
                Rem(block) => EntityModification::Remove { key, block },
                InsC(block, end) => EntityModification::Insert {
                    key,
                    data: entity! { SCHEMA => id: "one", count: block },
                    block,
                    end: Some(end),
                },
                OvwC(block, end) => EntityModification::Overwrite {
                    key,
                    data: entity! { SCHEMA => id: "one", count: block },
                    block,
                    end: Some(end),
                },
            }
        }
    }

    /// Helper to construct a `RowGroup`
    #[derive(Debug)]
    struct Group {
        group: RowGroup,
    }

    impl Group {
        fn new() -> Self {
            Self {
                group: RowGroup::new(THING_TYPE.clone(), false),
            }
        }

        fn append(&mut self, mods: &[Mod]) -> Result<(), StoreError> {
            for m in mods {
                self.group.append_row(EntityModification::from(m))?
            }
            Ok(())
        }

        fn with(mods: &[Mod]) -> Result<Group, StoreError> {
            let mut group = Self::new();
            group.append(mods)?;
            Ok(group)
        }
    }

    impl PartialEq<&[Mod]> for Group {
        fn eq(&self, mods: &&[Mod]) -> bool {
            let mods: Vec<_> = mods.iter().map(|m| EntityModification::from(m)).collect();
            self.group.rows == mods
        }
    }

    #[test]
    fn append() {
        use Mod::*;

        let res = Group::with(&[Ins(1), Ins(2)]);
        assert!(res.is_err());

        let res = Group::with(&[Ovw(1), Ins(2)]);
        assert!(res.is_err());

        let res = Group::with(&[Ins(1), Rem(2), Rem(3)]);
        assert!(res.is_err());

        let res = Group::with(&[Ovw(1), Rem(2), Rem(3)]);
        assert!(res.is_err());

        let res = Group::with(&[Ovw(1), Rem(2), Ovw(3)]);
        assert!(res.is_err());

        let group = Group::with(&[Ins(1), Ovw(2), Rem(3)]).unwrap();
        assert_eq!(group, &[InsC(1, 2), InsC(2, 3)]);

        let group = Group::with(&[Ovw(1), Rem(4)]).unwrap();
        assert_eq!(group, &[OvwC(1, 4)]);

        let group = Group::with(&[Ins(1), Rem(4)]).unwrap();
        assert_eq!(group, &[InsC(1, 4)]);

        let group = Group::with(&[Ins(1), Rem(2), Ins(3)]).unwrap();
        assert_eq!(group, &[InsC(1, 2), Ins(3)]);

        let group = Group::with(&[Ovw(1), Rem(2), Ins(3)]).unwrap();
        assert_eq!(group, &[OvwC(1, 2), Ins(3)]);
    }

    #[test]
    fn last_op() {
        #[track_caller]
        fn is_remove(group: &RowGroup, at: BlockNumber) {
            let key = THING_TYPE.parse_key("one").unwrap();
            let op = group.last_op(&key, at).unwrap();

            assert!(
                matches!(op, EntityOp::Remove { .. }),
                "op must be a remove at {} but is {:?}",
                at,
                op
            );
        }
        #[track_caller]
        fn is_write(group: &RowGroup, at: BlockNumber) {
            let key = THING_TYPE.parse_key("one").unwrap();
            let op = group.last_op(&key, at).unwrap();

            assert!(
                matches!(op, EntityOp::Write { .. }),
                "op must be a write at {} but is {:?}",
                at,
                op
            );
        }

        use Mod::*;

        let key = THING_TYPE.parse_key("one").unwrap();

        // This will result in two mods int the group:
        //   [ InsC(1,2), InsC(2,3) ]
        let group = Group::with(&[Ins(1), Ovw(2), Rem(3)]).unwrap().group;

        is_remove(&group, 5);
        is_remove(&group, 4);
        is_remove(&group, 3);

        is_write(&group, 2);
        is_write(&group, 1);

        let op = group.last_op(&key, 0);
        assert_eq!(None, op);
    }
}
