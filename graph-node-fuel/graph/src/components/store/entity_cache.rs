use anyhow::anyhow;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{self, Debug};
use std::sync::Arc;

use crate::components::store::write::EntityModification;
use crate::components::store::{self as s, Entity, EntityOperation};
use crate::data::store::{EntityValidationError, Id, IdType, IntoEntityIterator};
use crate::prelude::ENV_VARS;
use crate::schema::{EntityKey, InputSchema};
use crate::util::intern::Error as InternError;
use crate::util::lfu_cache::{EvictStats, LfuCache};

use super::{BlockNumber, DerivedEntityQuery, LoadRelatedRequest, StoreError};

/// The scope in which the `EntityCache` should perform a `get` operation
pub enum GetScope {
    /// Get from all previously stored entities in the store
    Store,
    /// Get from the entities that have been stored during this block
    InBlock,
}

/// A representation of entity operations that can be accumulated.
#[derive(Debug, Clone)]
enum EntityOp {
    Remove,
    Update(Entity),
    Overwrite(Entity),
}

impl EntityOp {
    fn apply_to(self, entity: &mut Option<Cow<Entity>>) -> Result<(), InternError> {
        use EntityOp::*;
        match (self, entity) {
            (Remove, e @ _) => *e = None,
            (Overwrite(new), e @ _) | (Update(new), e @ None) => *e = Some(Cow::Owned(new)),
            (Update(updates), Some(entity)) => entity.to_mut().merge_remove_null_fields(updates)?,
        }
        Ok(())
    }

    fn accumulate(&mut self, next: EntityOp) {
        use EntityOp::*;
        let update = match next {
            // Remove and Overwrite ignore the current value.
            Remove | Overwrite(_) => {
                *self = next;
                return;
            }
            Update(update) => update,
        };

        // We have an update, apply it.
        match self {
            // This is how `Overwrite` is constructed, by accumulating `Update` onto `Remove`.
            Remove => *self = Overwrite(update),
            Update(current) | Overwrite(current) => current.merge(update),
        }
    }
}

/// A cache for entities from the store that provides the basic functionality
/// needed for the store interactions in the host exports. This struct tracks
/// how entities are modified, and caches all entities looked up from the
/// store. The cache makes sure that
///   (1) no entity appears in more than one operation
///   (2) only entities that will actually be changed from what they
///       are in the store are changed
///
/// It is important for correctness that this struct is newly instantiated
/// at every block using `with_current` to seed the cache.
pub struct EntityCache {
    /// The state of entities in the store. An entry of `None`
    /// means that the entity is not present in the store
    current: LfuCache<EntityKey, Option<Entity>>,

    /// The accumulated changes to an entity.
    updates: HashMap<EntityKey, EntityOp>,

    // Updates for a currently executing handler.
    handler_updates: HashMap<EntityKey, EntityOp>,

    // Marks whether updates should go in `handler_updates`.
    in_handler: bool,

    /// The store is only used to read entities.
    pub store: Arc<dyn s::ReadStore>,

    pub schema: InputSchema,

    /// A sequence number for generating entity IDs. We use one number for
    /// all id's as the id's are scoped by block and a u32 has plenty of
    /// room for all changes in one block. To ensure reproducability of
    /// generated IDs, the `EntityCache` needs to be newly instantiated for
    /// each block
    seq: u32,
}

impl Debug for EntityCache {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("EntityCache")
            .field("current", &self.current)
            .field("updates", &self.updates)
            .finish()
    }
}

pub struct ModificationsAndCache {
    pub modifications: Vec<s::EntityModification>,
    pub entity_lfu_cache: LfuCache<EntityKey, Option<Entity>>,
    pub evict_stats: EvictStats,
}

impl EntityCache {
    pub fn new(store: Arc<dyn s::ReadStore>) -> Self {
        Self {
            current: LfuCache::new(),
            updates: HashMap::new(),
            handler_updates: HashMap::new(),
            in_handler: false,
            schema: store.input_schema(),
            store,
            seq: 0,
        }
    }

    /// Make a new entity. The entity is not part of the cache
    pub fn make_entity<I: IntoEntityIterator>(
        &self,
        iter: I,
    ) -> Result<Entity, EntityValidationError> {
        self.schema.make_entity(iter)
    }

    pub fn with_current(
        store: Arc<dyn s::ReadStore>,
        current: LfuCache<EntityKey, Option<Entity>>,
    ) -> EntityCache {
        EntityCache {
            current,
            updates: HashMap::new(),
            handler_updates: HashMap::new(),
            in_handler: false,
            schema: store.input_schema(),
            store,
            seq: 0,
        }
    }

    pub(crate) fn enter_handler(&mut self) {
        assert!(!self.in_handler);
        self.in_handler = true;
    }

    pub(crate) fn exit_handler(&mut self) {
        assert!(self.in_handler);
        self.in_handler = false;

        // Apply all handler updates to the main `updates`.
        let handler_updates = Vec::from_iter(self.handler_updates.drain());
        for (key, op) in handler_updates {
            self.entity_op(key, op)
        }
    }

    pub(crate) fn exit_handler_and_discard_changes(&mut self) {
        assert!(self.in_handler);
        self.in_handler = false;
        self.handler_updates.clear();
    }

    pub fn get(
        &mut self,
        key: &EntityKey,
        scope: GetScope,
    ) -> Result<Option<Cow<Entity>>, StoreError> {
        // Get the current entity, apply any updates from `updates`, then
        // from `handler_updates`.
        let mut entity: Option<Cow<Entity>> = match scope {
            GetScope::Store => {
                if !self.current.contains_key(key) {
                    let entity = self.store.get(key)?;
                    self.current.insert(key.clone(), entity);
                }
                // Unwrap: we just inserted the entity
                self.current.get(key).unwrap().as_ref().map(Cow::Borrowed)
            }
            GetScope::InBlock => None,
        };

        // Always test the cache consistency in debug mode. The test only
        // makes sense when we were actually asked to read from the store
        debug_assert!(match scope {
            GetScope::Store => entity == self.store.get(key).unwrap().map(Cow::Owned),
            GetScope::InBlock => true,
        });

        if let Some(op) = self.updates.get(key).cloned() {
            op.apply_to(&mut entity)
                .map_err(|e| key.unknown_attribute(e))?;
        }
        if let Some(op) = self.handler_updates.get(key).cloned() {
            op.apply_to(&mut entity)
                .map_err(|e| key.unknown_attribute(e))?;
        }
        Ok(entity)
    }

    pub fn load_related(
        &mut self,
        eref: &LoadRelatedRequest,
    ) -> Result<Vec<Entity>, anyhow::Error> {
        let (entity_type, field) = self.schema.get_field_related(eref)?;

        let query = DerivedEntityQuery {
            entity_type,
            entity_field: field.name.clone().into(),
            value: eref.entity_id.clone(),
            causality_region: eref.causality_region,
        };

        let mut entity_map = self.store.get_derived(&query)?;

        for (key, entity) in entity_map.iter() {
            // Only insert to the cache if it's not already there
            if !self.current.contains_key(&key) {
                self.current.insert(key.clone(), Some(entity.clone()));
            }
        }

        let mut keys_to_remove = Vec::new();

        // Apply updates from `updates` and `handler_updates` directly to entities in `entity_map` that match the query
        for (key, entity) in entity_map.iter_mut() {
            let mut entity_cow = Some(Cow::Borrowed(entity));

            if let Some(op) = self.updates.get(key).cloned() {
                op.apply_to(&mut entity_cow)
                    .map_err(|e| key.unknown_attribute(e))?;
            }

            if let Some(op) = self.handler_updates.get(key).cloned() {
                op.apply_to(&mut entity_cow)
                    .map_err(|e| key.unknown_attribute(e))?;
            }

            if let Some(updated_entity) = entity_cow {
                *entity = updated_entity.into_owned();
            } else {
                // if entity_cow is None, it means that the entity was removed by an update
                // mark the key for removal from the map
                keys_to_remove.push(key.clone());
            }
        }

        // A helper function that checks if an update matches the query and returns the updated entity if it does
        fn matches_query(
            op: &EntityOp,
            query: &DerivedEntityQuery,
            key: &EntityKey,
        ) -> Result<Option<Entity>, anyhow::Error> {
            match op {
                EntityOp::Update(entity) | EntityOp::Overwrite(entity)
                    if query.matches(key, entity) =>
                {
                    Ok(Some(entity.clone()))
                }
                EntityOp::Remove => Ok(None),
                _ => Ok(None),
            }
        }

        // Iterate over self.updates to find entities that:
        // - Aren't already present in the entity_map
        // - Match the query
        // If these conditions are met:
        // - Check if there's an update for the same entity in handler_updates and apply it.
        // - Add the entity to entity_map.
        for (key, op) in self.updates.iter() {
            if !entity_map.contains_key(key) {
                if let Some(entity) = matches_query(op, &query, key)? {
                    if let Some(handler_op) = self.handler_updates.get(key).cloned() {
                        // If there's a corresponding update in handler_updates, apply it to the entity
                        // and insert the updated entity into entity_map
                        let mut entity_cow = Some(Cow::Borrowed(&entity));
                        handler_op
                            .apply_to(&mut entity_cow)
                            .map_err(|e| key.unknown_attribute(e))?;

                        if let Some(updated_entity) = entity_cow {
                            entity_map.insert(key.clone(), updated_entity.into_owned());
                        }
                    } else {
                        // If there isn't a corresponding update in handler_updates or the update doesn't match the query, just insert the entity from self.updates
                        entity_map.insert(key.clone(), entity);
                    }
                }
            }
        }

        // Iterate over handler_updates to find entities that:
        // - Aren't already present in the entity_map.
        // - Aren't present in self.updates.
        // - Match the query.
        // If these conditions are met, add the entity to entity_map.
        for (key, handler_op) in self.handler_updates.iter() {
            if !entity_map.contains_key(key) && !self.updates.contains_key(key) {
                if let Some(entity) = matches_query(handler_op, &query, key)? {
                    entity_map.insert(key.clone(), entity);
                }
            }
        }

        // Remove entities that are in the store but have been removed by an update.
        // We do this last since the loops over updates and handler_updates are only
        // concerned with entities that are not in the store yet and by leaving removed
        // keys in entity_map we avoid processing these updates a second time when we
        // already looked at them when we went through entity_map
        for key in keys_to_remove {
            entity_map.remove(&key);
        }

        Ok(entity_map.into_values().collect())
    }

    pub fn remove(&mut self, key: EntityKey) {
        self.entity_op(key, EntityOp::Remove);
    }

    /// Store the `entity` under the given `key`. The `entity` may be only a
    /// partial entity; the cache will ensure partial updates get merged
    /// with existing data. The entity will be validated against the
    /// subgraph schema, and any errors will result in an `Err` being
    /// returned.
    pub fn set(&mut self, key: EntityKey, entity: Entity) -> Result<(), anyhow::Error> {
        // check the validate for derived fields
        let is_valid = entity.validate(&key).is_ok();

        self.entity_op(key.clone(), EntityOp::Update(entity));

        // The updates we were given are not valid by themselves; force a
        // lookup in the database and check again with an entity that merges
        // the existing entity with the changes
        if !is_valid {
            let entity = self.get(&key, GetScope::Store)?.ok_or_else(|| {
                anyhow!(
                    "Failed to read entity {}[{}] back from cache",
                    key.entity_type,
                    key.entity_id
                )
            })?;
            entity.validate(&key)?;
        }

        Ok(())
    }

    pub fn append(&mut self, operations: Vec<EntityOperation>) {
        assert!(!self.in_handler);

        for operation in operations {
            match operation {
                EntityOperation::Set { key, data } => {
                    self.entity_op(key, EntityOp::Update(data));
                }
                EntityOperation::Remove { key } => {
                    self.entity_op(key, EntityOp::Remove);
                }
            }
        }
    }

    fn entity_op(&mut self, key: EntityKey, op: EntityOp) {
        use std::collections::hash_map::Entry;
        let updates = match self.in_handler {
            true => &mut self.handler_updates,
            false => &mut self.updates,
        };

        match updates.entry(key) {
            Entry::Vacant(entry) => {
                entry.insert(op);
            }
            Entry::Occupied(mut entry) => entry.get_mut().accumulate(op),
        }
    }

    pub(crate) fn extend(&mut self, other: EntityCache) {
        assert!(!other.in_handler);

        self.current.extend(other.current);
        for (key, op) in other.updates {
            self.entity_op(key, op);
        }
    }

    /// Generate an id.
    pub fn generate_id(&mut self, id_type: IdType, block: BlockNumber) -> anyhow::Result<Id> {
        let id = id_type.generate_id(block, self.seq)?;
        self.seq += 1;
        Ok(id)
    }

    /// Return the changes that have been made via `set` and `remove` as
    /// `EntityModification`, making sure to only produce one when a change
    /// to the current state is actually needed.
    ///
    /// Also returns the updated `LfuCache`.
    pub fn as_modifications(
        mut self,
        block: BlockNumber,
    ) -> Result<ModificationsAndCache, StoreError> {
        assert!(!self.in_handler);

        // The first step is to make sure all entities being set are in `self.current`.
        // For each subgraph, we need a map of entity type to missing entity ids.
        let missing = self
            .updates
            .keys()
            .filter(|key| !self.current.contains_key(key));

        // For immutable types, we assume that the subgraph is well-behaved,
        // and all updated immutable entities are in fact new, and skip
        // looking them up in the store. That ultimately always leads to an
        // `Insert` modification for immutable entities; if the assumption
        // is wrong and the store already has a version of the entity from a
        // previous block, the attempt to insert will trigger a constraint
        // violation in the database, ensuring correctness
        let missing = missing.filter(|key| !key.entity_type.is_immutable());

        for (entity_key, entity) in self.store.get_many(missing.cloned().collect())? {
            self.current.insert(entity_key, Some(entity));
        }

        let mut mods = Vec::new();
        for (key, update) in self.updates {
            use EntityModification::*;

            let current = self.current.remove(&key).and_then(|entity| entity);
            let modification = match (current, update) {
                // Entity was created
                (None, EntityOp::Update(mut updates))
                | (None, EntityOp::Overwrite(mut updates)) => {
                    updates.remove_null_fields();
                    self.current.insert(key.clone(), Some(updates.clone()));
                    Some(Insert {
                        key,
                        data: updates,
                        block,
                        end: None,
                    })
                }
                // Entity may have been changed
                (Some(current), EntityOp::Update(updates)) => {
                    let mut data = current.clone();
                    data.merge_remove_null_fields(updates)
                        .map_err(|e| key.unknown_attribute(e))?;
                    self.current.insert(key.clone(), Some(data.clone()));
                    if current != data {
                        Some(Overwrite {
                            key,
                            data,
                            block,
                            end: None,
                        })
                    } else {
                        None
                    }
                }
                // Entity was removed and then updated, so it will be overwritten
                (Some(current), EntityOp::Overwrite(data)) => {
                    self.current.insert(key.clone(), Some(data.clone()));
                    if current != data {
                        Some(Overwrite {
                            key,
                            data,
                            block,
                            end: None,
                        })
                    } else {
                        None
                    }
                }
                // Existing entity was deleted
                (Some(_), EntityOp::Remove) => {
                    self.current.insert(key.clone(), None);
                    Some(Remove { key, block })
                }
                // Entity was deleted, but it doesn't exist in the store
                (None, EntityOp::Remove) => None,
            };
            if let Some(modification) = modification {
                mods.push(modification)
            }
        }
        let evict_stats = self
            .current
            .evict_and_stats(ENV_VARS.mappings.entity_cache_size);

        Ok(ModificationsAndCache {
            modifications: mods,
            entity_lfu_cache: self.current,
            evict_stats,
        })
    }
}
