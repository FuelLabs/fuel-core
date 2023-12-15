use graph::blockchain::block_stream::FirehoseCursor;
use graph::components::store::{
    DeploymentCursorTracker, DerivedEntityQuery, GetScope, LoadRelatedRequest, ReadStore,
    StoredDynamicDataSource, WritableStore,
};
use graph::data::store::Id;
use graph::data::subgraph::schema::{DeploymentCreate, SubgraphError, SubgraphHealth};
use graph::data_source::CausalityRegion;
use graph::schema::{EntityKey, EntityType, InputSchema};
use graph::{
    components::store::{DeploymentId, DeploymentLocator},
    prelude::{DeploymentHash, Entity, EntityCache, EntityModification, Value},
};
use graph::{entity, prelude::*};
use hex_literal::hex;

use graph::semver::Version;
use lazy_static::lazy_static;
use slog::Logger;
use std::collections::{BTreeMap, BTreeSet};
use std::marker::PhantomData;
use std::sync::Arc;
use web3::types::H256;

use graph_store_postgres::SubgraphStore as DieselSubgraphStore;
use test_store::*;

lazy_static! {
    static ref SUBGRAPH_ID: DeploymentHash = DeploymentHash::new("entity_cache").unwrap();
    static ref DEPLOYMENT: DeploymentLocator =
        DeploymentLocator::new(DeploymentId::new(-12), SUBGRAPH_ID.clone());
    static ref SCHEMA: InputSchema = InputSchema::parse(
        "
            type Band @entity {
                id: ID!
                name: String!
                founded: Int
                label: String
            }
            ",
        SUBGRAPH_ID.clone(),
    )
    .expect("Test schema invalid");
}

struct MockStore {
    get_many_res: BTreeMap<EntityKey, Entity>,
}

impl MockStore {
    fn new(get_many_res: BTreeMap<EntityKey, Entity>) -> Self {
        Self { get_many_res }
    }
}

impl ReadStore for MockStore {
    fn get(&self, key: &EntityKey) -> Result<Option<Entity>, StoreError> {
        Ok(self.get_many_res.get(key).cloned())
    }

    fn get_many(
        &self,
        _keys: BTreeSet<EntityKey>,
    ) -> Result<BTreeMap<EntityKey, Entity>, StoreError> {
        Ok(self.get_many_res.clone())
    }

    fn get_derived(
        &self,
        _key: &DerivedEntityQuery,
    ) -> Result<BTreeMap<EntityKey, Entity>, StoreError> {
        Ok(self.get_many_res.clone())
    }

    fn input_schema(&self) -> InputSchema {
        SCHEMA.clone()
    }
}
impl DeploymentCursorTracker for MockStore {
    fn block_ptr(&self) -> Option<BlockPtr> {
        unimplemented!()
    }

    fn firehose_cursor(&self) -> FirehoseCursor {
        unimplemented!()
    }

    fn input_schema(&self) -> InputSchema {
        todo!()
    }
}

#[async_trait]
impl WritableStore for MockStore {
    async fn start_subgraph_deployment(&self, _: &Logger) -> Result<(), StoreError> {
        unimplemented!()
    }

    async fn revert_block_operations(
        &self,
        _: BlockPtr,
        _: FirehoseCursor,
    ) -> Result<(), StoreError> {
        unimplemented!()
    }

    async fn unfail_deterministic_error(
        &self,
        _: &BlockPtr,
        _: &BlockPtr,
    ) -> Result<UnfailOutcome, StoreError> {
        unimplemented!()
    }

    fn unfail_non_deterministic_error(&self, _: &BlockPtr) -> Result<UnfailOutcome, StoreError> {
        unimplemented!()
    }

    async fn fail_subgraph(&self, _: SubgraphError) -> Result<(), StoreError> {
        unimplemented!()
    }

    async fn supports_proof_of_indexing(&self) -> Result<bool, StoreError> {
        unimplemented!()
    }

    async fn transact_block_operations(
        &self,
        _: BlockPtr,
        _: FirehoseCursor,
        _: Vec<EntityModification>,
        _: &StopwatchMetrics,
        _: Vec<StoredDynamicDataSource>,
        _: Vec<SubgraphError>,
        _: Vec<StoredDynamicDataSource>,
        _: bool,
    ) -> Result<(), StoreError> {
        unimplemented!()
    }

    async fn is_deployment_synced(&self) -> Result<bool, StoreError> {
        unimplemented!()
    }

    fn unassign_subgraph(&self) -> Result<(), StoreError> {
        unimplemented!()
    }

    async fn load_dynamic_data_sources(
        &self,
        _manifest_idx_and_name: Vec<(u32, String)>,
    ) -> Result<Vec<StoredDynamicDataSource>, StoreError> {
        unimplemented!()
    }

    fn deployment_synced(&self) -> Result<(), StoreError> {
        unimplemented!()
    }

    fn shard(&self) -> &str {
        unimplemented!()
    }

    async fn health(&self) -> Result<SubgraphHealth, StoreError> {
        unimplemented!()
    }

    async fn flush(&self) -> Result<(), StoreError> {
        unimplemented!()
    }

    async fn causality_region_curr_val(&self) -> Result<Option<CausalityRegion>, StoreError> {
        unimplemented!()
    }

    async fn restart(self: Arc<Self>) -> Result<Option<Arc<dyn WritableStore>>, StoreError> {
        unimplemented!()
    }
}

fn make_band_key(id: &'static str) -> EntityKey {
    SCHEMA.entity_type("Band").unwrap().parse_key(id).unwrap()
}

fn sort_by_entity_key(mut mods: Vec<EntityModification>) -> Vec<EntityModification> {
    mods.sort_by_key(|m| m.key().clone());
    mods
}

#[tokio::test]
async fn empty_cache_modifications() {
    let store = Arc::new(MockStore::new(BTreeMap::new()));
    let cache = EntityCache::new(store);
    let result = cache.as_modifications(0);
    assert_eq!(result.unwrap().modifications, vec![]);
}

#[test]
fn insert_modifications() {
    // Return no entities from the store, forcing the cache to treat any `set`
    // operation as an insert.
    let store = MockStore::new(BTreeMap::new());

    let store = Arc::new(store);
    let mut cache = EntityCache::new(store);

    let mogwai_data = entity! { SCHEMA => id: "mogwai", name: "Mogwai" };
    let mogwai_key = make_band_key("mogwai");
    cache.set(mogwai_key.clone(), mogwai_data.clone()).unwrap();

    let sigurros_data = entity! { SCHEMA => id: "sigurros", name: "Sigur Ros" };
    let sigurros_key = make_band_key("sigurros");
    cache
        .set(sigurros_key.clone(), sigurros_data.clone())
        .unwrap();

    let result = cache.as_modifications(0);
    assert_eq!(
        sort_by_entity_key(result.unwrap().modifications),
        sort_by_entity_key(vec![
            EntityModification::insert(mogwai_key, mogwai_data, 0),
            EntityModification::insert(sigurros_key, sigurros_data, 0)
        ])
    );
}

fn entity_version_map(entity_type: &str, entities: Vec<Entity>) -> BTreeMap<EntityKey, Entity> {
    let mut map = BTreeMap::new();
    for entity in entities {
        let key = SCHEMA.entity_type(entity_type).unwrap().key(entity.id());
        map.insert(key, entity);
    }
    map
}

#[test]
fn overwrite_modifications() {
    // Pre-populate the store with entities so that the cache treats
    // every set operation as an overwrite.
    let store = {
        let entities = vec![
            entity! { SCHEMA => id: "mogwai", name: "Mogwai" },
            entity! { SCHEMA => id: "sigurros", name: "Sigur Ros" },
        ];
        MockStore::new(entity_version_map("Band", entities))
    };

    let store = Arc::new(store);
    let mut cache = EntityCache::new(store);

    let mogwai_data = entity! { SCHEMA => id: "mogwai", name: "Mogwai", founded: 1995 };
    let mogwai_key = make_band_key("mogwai");
    cache.set(mogwai_key.clone(), mogwai_data.clone()).unwrap();

    let sigurros_data = entity! { SCHEMA => id: "sigurros", name: "Sigur Ros", founded: 1994 };
    let sigurros_key = make_band_key("sigurros");
    cache
        .set(sigurros_key.clone(), sigurros_data.clone())
        .unwrap();

    let result = cache.as_modifications(0);
    assert_eq!(
        sort_by_entity_key(result.unwrap().modifications),
        sort_by_entity_key(vec![
            EntityModification::overwrite(mogwai_key, mogwai_data, 0),
            EntityModification::overwrite(sigurros_key, sigurros_data, 0)
        ])
    );
}

#[test]
fn consecutive_modifications() {
    // Pre-populate the store with data so that we can test setting a field to
    // `Value::Null`.
    let store = {
        let entities =
            vec![entity! { SCHEMA => id: "mogwai", name: "Mogwai", label: "Chemikal Underground" }];

        MockStore::new(entity_version_map("Band", entities))
    };

    let store = Arc::new(store);
    let mut cache = EntityCache::new(store);

    // First, add "founded" and change the "label".
    let update_data =
        entity! { SCHEMA => id: "mogwai", founded: 1995, label: "Rock Action Records" };
    let update_key = make_band_key("mogwai");
    cache.set(update_key, update_data).unwrap();

    // Then, just reset the "label".
    let update_data = entity! { SCHEMA => id: "mogwai", label: Value::Null };
    let update_key = make_band_key("mogwai");
    cache.set(update_key.clone(), update_data).unwrap();

    // We expect a single overwrite modification for the above that leaves "id"
    // and "name" untouched, sets "founded" and removes the "label" field.
    let result = cache.as_modifications(0);
    assert_eq!(
        sort_by_entity_key(result.unwrap().modifications),
        sort_by_entity_key(vec![EntityModification::overwrite(
            update_key,
            entity! { SCHEMA => id: "mogwai", name: "Mogwai", founded: 1995 },
            0
        )])
    );
}

const ACCOUNT_GQL: &str = "
    type Account @entity {
        id: ID!
        name: String!
        email: String!
        age: Int!
        wallets: [Wallet!]! @derivedFrom(field: \"account\")
    }

    type Wallet @entity {
        id: ID!
        balance: Int!
        account: Account!
    }
";

const ACCOUNT: &str = "Account";
const WALLET: &str = "Wallet";

lazy_static! {
    static ref LOAD_RELATED_ID_STRING: String = String::from("loadrelatedsubgraph");
    static ref LOAD_RELATED_ID: DeploymentHash =
        DeploymentHash::new(LOAD_RELATED_ID_STRING.as_str()).unwrap();
    static ref LOAD_RELATED_SUBGRAPH: InputSchema =
        InputSchema::parse(ACCOUNT_GQL, LOAD_RELATED_ID.clone())
            .expect("Failed to parse user schema");
    static ref TEST_BLOCK_1_PTR: BlockPtr = (
        H256::from(hex!(
            "8511fa04b64657581e3f00e14543c1d522d5d7e771b54aa3060b662ade47da13"
        )),
        1u64
    )
        .into();
    static ref WALLET_TYPE: EntityType = LOAD_RELATED_SUBGRAPH.entity_type(WALLET).unwrap();
    static ref ACCOUNT_TYPE: EntityType = LOAD_RELATED_SUBGRAPH.entity_type(ACCOUNT).unwrap();
}

fn remove_test_data(store: Arc<DieselSubgraphStore>) {
    store
        .delete_all_entities_for_test_use_only()
        .expect("deleting test entities succeeds");
}

fn run_store_test<R, F>(test: F)
where
    F: FnOnce(
            EntityCache,
            Arc<DieselSubgraphStore>,
            DeploymentLocator,
            Arc<dyn WritableStore>,
        ) -> R
        + Send
        + 'static,
    R: std::future::Future<Output = ()> + Send + 'static,
{
    run_test_sequentially(|store| async move {
        let subgraph_store = store.subgraph_store();
        // Reset state before starting
        remove_test_data(subgraph_store.clone());

        // Seed database with test data
        let deployment = insert_test_data(subgraph_store.clone()).await;
        let writable = store
            .subgraph_store()
            .writable(LOGGER.clone(), deployment.id, Arc::new(Vec::new()))
            .await
            .expect("we can get a writable store");

        // we send the information to the database
        writable.flush().await.unwrap();

        let read_store = Arc::new(writable.clone());

        let cache = EntityCache::new(read_store);
        // Run test and wait for the background writer to finish its work so
        // it won't conflict with the next test
        test(cache, subgraph_store.clone(), deployment, writable.clone()).await;
        writable.flush().await.unwrap();
    });
}

async fn insert_test_data(store: Arc<DieselSubgraphStore>) -> DeploymentLocator {
    let manifest = SubgraphManifest::<graph_chain_ethereum::Chain> {
        id: LOAD_RELATED_ID.clone(),
        spec_version: Version::new(1, 0, 0),
        features: Default::default(),
        description: None,
        repository: None,
        schema: LOAD_RELATED_SUBGRAPH.clone(),
        data_sources: vec![],
        graft: None,
        templates: vec![],
        chain: PhantomData,
        indexer_hints: None,
    };

    // Create SubgraphDeploymentEntity
    let deployment = DeploymentCreate::new(String::new(), &manifest, None);
    let name = SubgraphName::new("test/store").unwrap();
    let node_id = NodeId::new("test").unwrap();
    let deployment = store
        .create_subgraph_deployment(
            name,
            &LOAD_RELATED_SUBGRAPH,
            deployment,
            node_id,
            NETWORK_NAME.to_string(),
            SubgraphVersionSwitchingMode::Instant,
        )
        .unwrap();

    // 1 account 3 wallets
    let test_entity_1 = create_account_entity("1", "Johnton", "tonofjohn@email.com", 67_i32);
    let id_one = WALLET_TYPE.parse_id("1").unwrap();
    let wallet_entity_1 = create_wallet_operation("1", &id_one, 67_i32);
    let wallet_entity_2 = create_wallet_operation("2", &id_one, 92_i32);
    let wallet_entity_3 = create_wallet_operation("3", &id_one, 192_i32);
    // 1 account 1 wallet
    let test_entity_2 = create_account_entity("2", "Cindini", "dinici@email.com", 42_i32);
    let id_two = WALLET_TYPE.parse_id("2").unwrap();
    let wallet_entity_4 = create_wallet_operation("4", &id_two, 32_i32);
    // 1 account 0 wallets
    let test_entity_3 = create_account_entity("3", "Shaqueeena", "queensha@email.com", 28_i32);
    transact_entity_operations(
        &store,
        &deployment,
        GENESIS_PTR.clone(),
        vec![
            test_entity_1,
            test_entity_2,
            test_entity_3,
            wallet_entity_1,
            wallet_entity_2,
            wallet_entity_3,
            wallet_entity_4,
        ],
    )
    .await
    .unwrap();
    deployment
}

fn create_account_entity(id: &str, name: &str, email: &str, age: i32) -> EntityOperation {
    let test_entity =
        entity! { LOAD_RELATED_SUBGRAPH => id: id, name: name, email: email, age: age };

    EntityOperation::Set {
        key: ACCOUNT_TYPE.parse_key(id).unwrap(),
        data: test_entity,
    }
}

fn create_wallet_entity(id: &str, account_id: &Id, balance: i32) -> Entity {
    let account_id = Value::from(account_id.clone());
    entity! { LOAD_RELATED_SUBGRAPH => id: id, account: account_id, balance: balance }
}
fn create_wallet_operation(id: &str, account_id: &Id, balance: i32) -> EntityOperation {
    let test_wallet = create_wallet_entity(id, account_id, balance);
    EntityOperation::Set {
        key: WALLET_TYPE.parse_key(id).unwrap(),
        data: test_wallet,
    }
}

#[test]
fn check_for_account_with_multiple_wallets() {
    run_store_test(|mut cache, _store, _deployment, _writable| async move {
        let account_id = ACCOUNT_TYPE.parse_id("1").unwrap();
        let request = LoadRelatedRequest {
            entity_type: ACCOUNT_TYPE.clone(),
            entity_field: "wallets".into(),
            entity_id: account_id.clone(),
            causality_region: CausalityRegion::ONCHAIN,
        };
        let result = cache.load_related(&request).unwrap();
        let wallet_1 = create_wallet_entity("1", &account_id, 67_i32);
        let wallet_2 = create_wallet_entity("2", &account_id, 92_i32);
        let wallet_3 = create_wallet_entity("3", &account_id, 192_i32);
        let expeted_vec = vec![wallet_1, wallet_2, wallet_3];

        assert_eq!(result, expeted_vec);
    });
}

#[test]
fn check_for_account_with_single_wallet() {
    run_store_test(|mut cache, _store, _deployment, _writable| async move {
        let account_id = ACCOUNT_TYPE.parse_id("2").unwrap();
        let request = LoadRelatedRequest {
            entity_type: ACCOUNT_TYPE.clone(),
            entity_field: "wallets".into(),
            entity_id: account_id.clone(),
            causality_region: CausalityRegion::ONCHAIN,
        };
        let result = cache.load_related(&request).unwrap();
        let wallet_1 = create_wallet_entity("4", &account_id, 32_i32);
        let expeted_vec = vec![wallet_1];

        assert_eq!(result, expeted_vec);
    });
}

#[test]
fn check_for_account_with_no_wallet() {
    run_store_test(|mut cache, _store, _deployment, _writable| async move {
        let account_id = ACCOUNT_TYPE.parse_id("3").unwrap();
        let request = LoadRelatedRequest {
            entity_type: ACCOUNT_TYPE.clone(),
            entity_field: "wallets".into(),
            entity_id: account_id,
            causality_region: CausalityRegion::ONCHAIN,
        };
        let result = cache.load_related(&request).unwrap();
        let expeted_vec = vec![];

        assert_eq!(result, expeted_vec);
    });
}

#[test]
fn check_for_account_that_doesnt_exist() {
    run_store_test(|mut cache, _store, _deployment, _writable| async move {
        let account_id = ACCOUNT_TYPE.parse_id("4").unwrap();
        let request = LoadRelatedRequest {
            entity_type: ACCOUNT_TYPE.clone(),
            entity_field: "wallets".into(),
            entity_id: account_id,
            causality_region: CausalityRegion::ONCHAIN,
        };
        let result = cache.load_related(&request).unwrap();
        let expeted_vec = vec![];

        assert_eq!(result, expeted_vec);
    });
}

#[test]
fn check_for_non_existent_field() {
    run_store_test(|mut cache, _store, _deployment, _writable| async move {
        let account_id = ACCOUNT_TYPE.parse_id("1").unwrap();
        let request = LoadRelatedRequest {
            entity_type: ACCOUNT_TYPE.clone(),
            entity_field: "friends".into(),
            entity_id: account_id,
            causality_region: CausalityRegion::ONCHAIN,
        };
        let result = cache.load_related(&request).unwrap_err();
        let expected = format!(
            "Entity {}[{}]: unknown field `{}`",
            request.entity_type, request.entity_id, request.entity_field,
        );

        assert_eq!(format!("{}", result), expected);
    });
}

#[test]
fn check_for_insert_async_store() {
    run_store_test(|mut cache, store, deployment, _writable| async move {
        let account_id = ACCOUNT_TYPE.parse_id("2").unwrap();
        // insert a new wallet
        let wallet_entity_5 = create_wallet_operation("5", &account_id, 79_i32);
        let wallet_entity_6 = create_wallet_operation("6", &account_id, 200_i32);

        transact_entity_operations(
            &store,
            &deployment,
            TEST_BLOCK_1_PTR.clone(),
            vec![wallet_entity_5, wallet_entity_6],
        )
        .await
        .unwrap();
        let request = LoadRelatedRequest {
            entity_type: ACCOUNT_TYPE.clone(),
            entity_field: "wallets".into(),
            entity_id: account_id.clone(),
            causality_region: CausalityRegion::ONCHAIN,
        };
        let result = cache.load_related(&request).unwrap();
        let wallet_1 = create_wallet_entity("4", &account_id, 32_i32);
        let wallet_2 = create_wallet_entity("5", &account_id, 79_i32);
        let wallet_3 = create_wallet_entity("6", &account_id, 200_i32);
        let expeted_vec = vec![wallet_1, wallet_2, wallet_3];

        assert_eq!(result, expeted_vec);
    });
}
#[test]
fn check_for_insert_async_not_related() {
    run_store_test(|mut cache, store, deployment, _writable| async move {
        let account_id = ACCOUNT_TYPE.parse_id("2").unwrap();
        // insert a new wallet
        let wallet_entity_5 = create_wallet_operation("5", &account_id, 79_i32);
        let wallet_entity_6 = create_wallet_operation("6", &account_id, 200_i32);

        transact_entity_operations(
            &store,
            &deployment,
            TEST_BLOCK_1_PTR.clone(),
            vec![wallet_entity_5, wallet_entity_6],
        )
        .await
        .unwrap();
        let account_id = ACCOUNT_TYPE.parse_id("1").unwrap();
        let request = LoadRelatedRequest {
            entity_type: ACCOUNT_TYPE.clone(),
            entity_field: "wallets".into(),
            entity_id: account_id.clone(),
            causality_region: CausalityRegion::ONCHAIN,
        };
        let result = cache.load_related(&request).unwrap();
        let wallet_1 = create_wallet_entity("1", &account_id, 67_i32);
        let wallet_2 = create_wallet_entity("2", &account_id, 92_i32);
        let wallet_3 = create_wallet_entity("3", &account_id, 192_i32);
        let expeted_vec = vec![wallet_1, wallet_2, wallet_3];

        assert_eq!(result, expeted_vec);
    });
}

#[test]
fn check_for_update_async_related() {
    run_store_test(|mut cache, store, deployment, writable| async move {
        let entity_key = WALLET_TYPE.parse_key("1").unwrap();
        let account_id = entity_key.entity_id.clone();
        let wallet_entity_update = create_wallet_operation("1", &account_id, 79_i32);

        let new_data = match wallet_entity_update {
            EntityOperation::Set { ref data, .. } => data.clone(),
            _ => unreachable!(),
        };
        assert_ne!(writable.get(&entity_key).unwrap().unwrap(), new_data);
        // insert a new wallet
        transact_entity_operations(
            &store,
            &deployment,
            TEST_BLOCK_1_PTR.clone(),
            vec![wallet_entity_update],
        )
        .await
        .unwrap();

        let request = LoadRelatedRequest {
            entity_type: ACCOUNT_TYPE.clone(),
            entity_field: "wallets".into(),
            entity_id: account_id.clone(),
            causality_region: CausalityRegion::ONCHAIN,
        };
        let result = cache.load_related(&request).unwrap();
        let wallet_2 = create_wallet_entity("2", &account_id, 92_i32);
        let wallet_3 = create_wallet_entity("3", &account_id, 192_i32);
        let expeted_vec = vec![new_data, wallet_2, wallet_3];

        assert_eq!(result, expeted_vec);
    });
}

#[test]
fn check_for_delete_async_related() {
    run_store_test(|mut cache, store, deployment, _writable| async move {
        let account_id = ACCOUNT_TYPE.parse_id("1").unwrap();
        let del_key = WALLET_TYPE.parse_key("1").unwrap();
        // delete wallet
        transact_entity_operations(
            &store,
            &deployment,
            TEST_BLOCK_1_PTR.clone(),
            vec![EntityOperation::Remove { key: del_key }],
        )
        .await
        .unwrap();

        let request = LoadRelatedRequest {
            entity_type: ACCOUNT_TYPE.clone(),
            entity_field: "wallets".into(),
            entity_id: account_id.clone(),
            causality_region: CausalityRegion::ONCHAIN,
        };
        let result = cache.load_related(&request).unwrap();
        let wallet_2 = create_wallet_entity("2", &account_id, 92_i32);
        let wallet_3 = create_wallet_entity("3", &account_id, 192_i32);
        let expeted_vec = vec![wallet_2, wallet_3];

        assert_eq!(result, expeted_vec);
    });
}

#[test]
fn scoped_get() {
    run_store_test(|mut cache, _store, _deployment, _writable| async move {
        // Key for an existing entity that is in the store
        let account1 = ACCOUNT_TYPE.parse_id("1").unwrap();
        let key1 = WALLET_TYPE.parse_key("1").unwrap();
        let wallet1 = create_wallet_entity("1", &account1, 67);

        // Create a new entity that is not in the store
        let account5 = ACCOUNT_TYPE.parse_id("5").unwrap();
        let wallet5 = create_wallet_entity("5", &account5, 100);
        let key5 = WALLET_TYPE.parse_key("5").unwrap();
        cache.set(key5.clone(), wallet5.clone()).unwrap();

        // For the new entity, we can retrieve it with either scope
        let act5 = cache.get(&key5, GetScope::InBlock).unwrap();
        assert_eq!(Some(&wallet5), act5.as_ref().map(|e| e.as_ref()));
        let act5 = cache.get(&key5, GetScope::Store).unwrap();
        assert_eq!(Some(&wallet5), act5.as_ref().map(|e| e.as_ref()));

        // For an entity in the store, we can not get it `InBlock` but with
        // `Store`
        let act1 = cache.get(&key1, GetScope::InBlock).unwrap();
        assert_eq!(None, act1);
        let act1 = cache.get(&key1, GetScope::Store).unwrap();
        assert_eq!(Some(&wallet1), act1.as_ref().map(|e| e.as_ref()));
        // Even after reading from the store, the entity is not visible with
        // `InBlock`
        let act1 = cache.get(&key1, GetScope::InBlock).unwrap();
        assert_eq!(None, act1);
        // But if it gets updated, it becomes visible with either scope
        let mut wallet1 = wallet1;
        wallet1.set("balance", 70).unwrap();
        cache.set(key1.clone(), wallet1.clone()).unwrap();
        let act1 = cache.get(&key1, GetScope::InBlock).unwrap();
        assert_eq!(Some(&wallet1), act1.as_ref().map(|e| e.as_ref()));
        let act1 = cache.get(&key1, GetScope::Store).unwrap();
        assert_eq!(Some(&wallet1), act1.as_ref().map(|e| e.as_ref()));
    })
}

/// Entities should never contain a `__typename` or `g$parent_id` field, if
/// they do, that can cause PoI divergences, because entities will differ
/// depending on whether they had to be loaded from the database or stuck
/// around in the cache where they won't have these attributes
#[test]
fn no_internal_keys() {
    run_store_test(|mut cache, _, _, writable| async move {
        #[track_caller]
        fn check(key: &EntityKey, entity: &Entity) {
            // Validate checks that all attributes are actually declared in
            // the schema
            entity.validate(key).expect("the entity is valid");
        }
        let key = WALLET_TYPE.parse_key("1").unwrap();

        let wallet = writable.get(&key).unwrap().unwrap();
        check(&key, &wallet);

        let wallet = cache.get(&key, GetScope::Store).unwrap().unwrap();
        check(&key, &wallet);
    });
}
