use graph::blockchain::block_stream::FirehoseCursor;
use graph::data::graphql::ext::TypeDefinitionExt;
use graph::data::query::QueryTarget;
use graph::data::subgraph::schema::DeploymentCreate;
use graph::schema::{EntityType, InputSchema};
use graph_chain_ethereum::{Mapping, MappingABI};
use hex_literal::hex;
use lazy_static::lazy_static;
use std::time::Duration;
use std::{collections::HashSet, sync::Mutex};
use std::{marker::PhantomData, str::FromStr};
use test_store::*;

use graph::components::store::{DeploymentLocator, ReadStore, WritableStore};
use graph::data::subgraph::*;
use graph::{
    blockchain::DataSource,
    components::store::{
        BlockStore as _, EntityFilter, EntityOrder, EntityQuery, StatusStore,
        SubscriptionManager as _,
    },
    prelude::ethabi::Contract,
};
use graph::{data::store::scalar, semver::Version};
use graph::{entity, prelude::*};
use graph_store_postgres::layout_for_tests::STRING_PREFIX_SIZE;
use graph_store_postgres::{Store as DieselStore, SubgraphStore as DieselSubgraphStore};
use web3::types::{Address, H256};

const USER_GQL: &str = "
    interface ColorAndAge {
        id: ID!,
        age: Int,
        favorite_color: String
    }

    type User implements ColorAndAge @entity {
        id: ID!,
        name: String,
        bin_name: Bytes,
        email: String,
        age: Int,
        seconds_age: BigInt,
        weight: BigDecimal,
        coffee: Boolean,
        favorite_color: String
    }

    type Person implements ColorAndAge @entity {
        id: ID!,
        name: String,
        age: Int,
        favorite_color: String
    }

    type Manual @entity {
        id: ID!,
        text: String
    }
";

const USER: &str = "User";

lazy_static! {
    static ref TEST_SUBGRAPH_ID_STRING: String = String::from("testsubgraph");
    static ref TEST_SUBGRAPH_ID: DeploymentHash =
        DeploymentHash::new(TEST_SUBGRAPH_ID_STRING.as_str()).unwrap();
    static ref TEST_SUBGRAPH_SCHEMA: InputSchema =
        InputSchema::parse(USER_GQL, TEST_SUBGRAPH_ID.clone())
            .expect("Failed to parse user schema");
    static ref TEST_BLOCK_0_PTR: BlockPtr = (
        H256::from(hex!(
            "bd34884280958002c51d3f7b5f853e6febeba33de0f40d15b0363006533c924f"
        )),
        0u64
    )
        .into();
    static ref TEST_BLOCK_1_PTR: BlockPtr = (
        H256::from(hex!(
            "8511fa04b64657581e3f00e14543c1d522d5d7e771b54aa3060b662ade47da13"
        )),
        1u64
    )
        .into();
    static ref TEST_BLOCK_2_PTR: BlockPtr = (
        H256::from(hex!(
            "b98fb783b49de5652097a989414c767824dff7e7fd765a63b493772511db81c1"
        )),
        2u64
    )
        .into();
    static ref TEST_BLOCK_3_PTR: BlockPtr = (
        H256::from(hex!(
            "977c084229c72a0fa377cae304eda9099b6a2cb5d83b25cdf0f0969b69874255"
        )),
        3u64
    )
        .into();
    static ref TEST_BLOCK_3A_PTR: BlockPtr = (
        H256::from(hex!(
            "d163aec0592c7cb00c2700ab65dcaac93289f5d250b3b889b39198b07e1fbe4a"
        )),
        3u64
    )
        .into();
    static ref TEST_BLOCK_4_PTR: BlockPtr = (
        H256::from(hex!(
            "007a03cdf635ebb66f5e79ae66cc90ca23d98031665649db056ff9c6aac2d74d"
        )),
        4u64
    )
        .into();
    static ref TEST_BLOCK_4A_PTR: BlockPtr = (
        H256::from(hex!(
            "8fab27e9e9285b0a39110f4d9877f05d0f43d2effa157e55f4dcc49c3cf8cbd7"
        )),
        4u64
    )
        .into();
    static ref TEST_BLOCK_5_PTR: BlockPtr = (
        H256::from(hex!(
            "e8b3b02b936c4a4a331ac691ac9a86e197fb7731f14e3108602c87d4dac55160"
        )),
        5u64
    )
        .into();
    static ref USER_TYPE: EntityType = TEST_SUBGRAPH_SCHEMA.entity_type(USER).unwrap();
    static ref PERSON_TYPE: EntityType = TEST_SUBGRAPH_SCHEMA.entity_type("Person").unwrap();
}

/// Test harness for running database integration tests.
fn run_test<R, F>(test: F)
where
    F: FnOnce(Arc<DieselStore>, Arc<dyn WritableStore>, DeploymentLocator) -> R + Send + 'static,
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

        // Run test and wait for the background writer to finish its work so
        // it won't conflict with the next test
        test(store, writable.clone(), deployment).await;
        writable.flush().await.unwrap();
    });
}

/// Inserts test data into the store.
///
/// Inserts data in test blocks `GENESIS_PTR`, `TEST_BLOCK_1_PTR`, and
/// `TEST_BLOCK_2_PTR`
async fn insert_test_data(store: Arc<DieselSubgraphStore>) -> DeploymentLocator {
    let manifest = SubgraphManifest::<graph_chain_ethereum::Chain> {
        id: TEST_SUBGRAPH_ID.clone(),
        spec_version: Version::new(1, 0, 0),
        features: Default::default(),
        description: None,
        repository: None,
        schema: TEST_SUBGRAPH_SCHEMA.clone(),
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
            &TEST_SUBGRAPH_SCHEMA,
            deployment,
            node_id,
            NETWORK_NAME.to_string(),
            SubgraphVersionSwitchingMode::Instant,
        )
        .unwrap();

    let test_entity_1 = create_test_entity(
        "1",
        &*USER_TYPE,
        "Johnton",
        "tonofjohn@email.com",
        67_i32,
        184.4,
        false,
        None,
    );
    transact_entity_operations(
        &store,
        &deployment,
        GENESIS_PTR.clone(),
        vec![test_entity_1],
    )
    .await
    .unwrap();

    let test_entity_2 = create_test_entity(
        "2",
        &*USER_TYPE,
        "Cindini",
        "dinici@email.com",
        43_i32,
        159.1,
        true,
        Some("red"),
    );
    let test_entity_3_1 = create_test_entity(
        "3",
        &*USER_TYPE,
        "Shaqueeena",
        "queensha@email.com",
        28_i32,
        111.7,
        false,
        Some("blue"),
    );
    transact_entity_operations(
        &store,
        &deployment,
        TEST_BLOCK_1_PTR.clone(),
        vec![test_entity_2, test_entity_3_1],
    )
    .await
    .unwrap();

    let test_entity_3_2 = create_test_entity(
        "3",
        &*USER_TYPE,
        "Shaqueeena",
        "teeko@email.com",
        28_i32,
        111.7,
        false,
        None,
    );
    transact_and_wait(
        &store,
        &deployment,
        TEST_BLOCK_2_PTR.clone(),
        vec![test_entity_3_2],
    )
    .await
    .unwrap();

    deployment
}

/// Creates a test entity.
fn create_test_entity(
    id: &str,
    entity_type: &EntityType,
    name: &str,
    email: &str,
    age: i32,
    weight: f64,
    coffee: bool,
    favorite_color: Option<&str>,
) -> EntityOperation {
    let bin_name = scalar::Bytes::from_str(&hex::encode(name)).unwrap();
    let test_entity = entity! { TEST_SUBGRAPH_SCHEMA =>
        id: id,
        name: name,
        bin_name: bin_name,
        email: email,
        age: age,
        seconds_age: Value::BigInt(BigInt::from(age) * 31557600.into()),
        weight: Value::BigDecimal(weight.into()),
        coffee: coffee,
        favorite_color: favorite_color,
    };

    EntityOperation::Set {
        key: entity_type.parse_key(id).unwrap(),
        data: test_entity,
    }
}

/// Removes test data from the database behind the store.
fn remove_test_data(store: Arc<DieselSubgraphStore>) {
    store
        .delete_all_entities_for_test_use_only()
        .expect("deleting test entities succeeds");
}

fn get_entity_count(store: Arc<DieselStore>, subgraph_id: &DeploymentHash) -> u64 {
    let info = store
        .status(status::Filter::Deployments(vec![subgraph_id.to_string()]))
        .unwrap();
    let info = info.first().unwrap();
    info.entity_count
}

#[test]
fn delete_entity() {
    run_test(|store, writable, deployment| async move {
        let entity_key = USER_TYPE.parse_key("3").unwrap();

        // Check that there is an entity to remove.
        writable.get(&entity_key).unwrap().unwrap();

        let count = get_entity_count(store.clone(), &deployment.hash);
        transact_and_wait(
            &store.subgraph_store(),
            &deployment,
            TEST_BLOCK_3_PTR.clone(),
            vec![EntityOperation::Remove {
                key: entity_key.clone(),
            }],
        )
        .await
        .unwrap();
        assert_eq!(count, get_entity_count(store.clone(), &deployment.hash) + 1);

        // Check that that the deleted entity id is not present
        assert!(writable.get(&entity_key).unwrap().is_none());
    })
}

/// Check that user 1 was inserted correctly
#[test]
fn get_entity_1() {
    run_test(|_, writable, _| async move {
        let schema = ReadStore::input_schema(&writable);

        let key = USER_TYPE.parse_key("1").unwrap();
        let result = writable.get(&key).unwrap();

        let bin_name = Value::Bytes("Johnton".as_bytes().into());
        let expected_entity = entity! { schema =>
            id: "1",
            name: "Johnton",
            bin_name: bin_name,
            email: "tonofjohn@email.com",
            age: 67_i32,
            seconds_age: Value::BigInt(BigInt::from(2114359200)),
            weight: Value::BigDecimal(184.4.into()),
            coffee: false,
        };
        // "favorite_color" was set to `Null` earlier and should be absent

        // Check that the expected entity was returned
        assert_eq!(result, Some(expected_entity));
    })
}

/// Check that user 3 was updated correctly
#[test]
fn get_entity_3() {
    run_test(|_, writable, _| async move {
        let schema = ReadStore::input_schema(&writable);
        let key = USER_TYPE.parse_key("3").unwrap();
        let result = writable.get(&key).unwrap();

        let expected_entity = entity! { schema =>
           id: "3",
           name: "Shaqueeena",
           bin_name: Value::Bytes("Shaqueeena".as_bytes().into()),
           email: "teeko@email.com",
           age: 28_i32,
           seconds_age: Value::BigInt(BigInt::from(883612800)),
           weight: Value::BigDecimal(111.7.into()),
           coffee: false,
        };
        // "favorite_color" was set to `Null` earlier and should be absent

        // Check that the expected entity was returned
        assert_eq!(result, Some(expected_entity));
    })
}

#[test]
fn insert_entity() {
    run_test(|store, writable, deployment| async move {
        let entity_key = USER_TYPE.parse_key("7").unwrap();
        let test_entity = create_test_entity(
            "7",
            &*USER_TYPE,
            "Wanjon",
            "wanawana@email.com",
            76_i32,
            111.7,
            true,
            Some("green"),
        );
        let count = get_entity_count(store.clone(), &deployment.hash);
        transact_and_wait(
            &store.subgraph_store(),
            &deployment,
            TEST_BLOCK_3_PTR.clone(),
            vec![test_entity],
        )
        .await
        .unwrap();
        assert_eq!(count + 1, get_entity_count(store.clone(), &deployment.hash));

        // Check that new record is in the store
        writable.get(&entity_key).unwrap().unwrap();
    })
}

#[test]
fn update_existing() {
    run_test(|store, writable, deployment| async move {
        let entity_key = USER_TYPE.parse_key("1").unwrap();

        let op = create_test_entity(
            "1",
            &*USER_TYPE,
            "Wanjon",
            "wanawana@email.com",
            76_i32,
            111.7,
            true,
            Some("green"),
        );
        let mut new_data = match op {
            EntityOperation::Set { ref data, .. } => data.clone(),
            _ => unreachable!(),
        };

        // Verify that the entity before updating is different from what we expect afterwards
        assert_ne!(writable.get(&entity_key).unwrap().unwrap(), new_data);

        // Set test entity; as the entity already exists an update should be performed
        let count = get_entity_count(store.clone(), &deployment.hash);
        transact_entity_operations(
            &store.subgraph_store(),
            &deployment,
            TEST_BLOCK_3_PTR.clone(),
            vec![op],
        )
        .await
        .unwrap();
        assert_eq!(count, get_entity_count(store.clone(), &deployment.hash));

        // Verify that the entity in the store has changed to what we have set.
        let bin_name = match new_data.get("bin_name") {
            Some(Value::Bytes(bytes)) => bytes.clone(),
            _ => unreachable!(),
        };

        new_data.insert("bin_name", Value::Bytes(bin_name)).unwrap();
        assert_eq!(writable.get(&entity_key).unwrap(), Some(new_data));
    })
}

#[test]
fn partially_update_existing() {
    run_test(|store, writable, deployment| async move {
        let entity_key = USER_TYPE.parse_key("1").unwrap();
        let schema = writable.input_schema();

        let partial_entity = entity! { schema => id: "1", name: "Johnny Boy", email: Value::Null };

        let original_entity = writable
            .get(&entity_key)
            .unwrap()
            .expect("entity not found");

        // Set test entity; as the entity already exists an update should be performed
        transact_entity_operations(
            &store.subgraph_store(),
            &deployment,
            TEST_BLOCK_3_PTR.clone(),
            vec![EntityOperation::Set {
                key: entity_key.clone(),
                data: partial_entity.clone(),
            }],
        )
        .await
        .unwrap();

        // Obtain the updated entity from the store
        let updated_entity = writable
            .get(&entity_key)
            .unwrap()
            .expect("entity not found");

        // Verify that the values of all attributes we have set were either unset
        // (in the case of Value::Null) or updated to the new values
        assert_eq!(updated_entity.get("id"), partial_entity.get("id"));
        assert_eq!(updated_entity.get(USER), partial_entity.get(USER));
        assert_eq!(updated_entity.get("email"), None);

        // Verify that all attributes we have not set have remained at their old values
        assert_eq!(updated_entity.get("age"), original_entity.get("age"));
        assert_eq!(updated_entity.get("weight"), original_entity.get("weight"));
        assert_eq!(updated_entity.get("coffee"), original_entity.get("coffee"));
    })
}

struct QueryChecker {
    store: Arc<DieselStore>,
}

impl QueryChecker {
    fn new(store: Arc<DieselStore>) -> Self {
        Self { store }
    }

    fn check(self, expected_entity_ids: Vec<&str>, query: EntityQuery) -> Self {
        let expected_entity_ids: Vec<String> =
            expected_entity_ids.into_iter().map(str::to_owned).collect();

        let entities = self
            .store
            .subgraph_store()
            .find(query)
            .expect("store.find failed to execute query");

        let entity_ids: Vec<_> = entities
            .into_iter()
            .map(|entity| match entity.get("id") {
                Some(Value::String(id)) => id.clone(),
                Some(_) => panic!("store.find returned entity with non-string ID attribute"),
                None => panic!("store.find returned entity with no ID attribute"),
            })
            .collect();

        assert_eq!(entity_ids, expected_entity_ids);
        self
    }
}

fn user_query() -> EntityQuery {
    EntityQuery::new(
        TEST_SUBGRAPH_ID.clone(),
        BLOCK_NUMBER_MAX,
        EntityCollection::All(vec![(USER_TYPE.clone(), AttributeNames::All)]),
    )
}

trait EasyOrder {
    fn asc(self, attr: &str) -> Self;
    fn desc(self, attr: &str) -> Self;
}

impl EasyOrder for EntityQuery {
    fn asc(self, attr: &str) -> Self {
        // The ValueType doesn't matter since relational layouts ignore it
        self.order(EntityOrder::Ascending(attr.to_owned(), ValueType::String))
    }

    fn desc(self, attr: &str) -> Self {
        // The ValueType doesn't matter since relational layouts ignore it
        self.order(EntityOrder::Descending(attr.to_owned(), ValueType::String))
    }
}

#[test]
fn find() {
    run_test(|store, _, _| async move {
        // Filter tests with string attributes
        QueryChecker::new(store.clone())
            .check(
                vec!["2"],
                user_query().filter(EntityFilter::Contains("name".into(), "ind".into())),
            )
            .check(
                vec!["2"],
                user_query().filter(EntityFilter::Equal("name".to_owned(), "Cindini".into())),
            )
            .check(
                vec!["1", "3"],
                user_query()
                    .filter(EntityFilter::Not("name".to_owned(), "Cindini".into()))
                    .asc("name"),
            )
            .check(
                vec!["3"],
                user_query().filter(EntityFilter::GreaterThan("name".to_owned(), "Kundi".into())),
            )
            .check(
                vec!["2", "1"],
                user_query()
                    .filter(EntityFilter::LessThan("name".to_owned(), "Kundi".into()))
                    .asc("name"),
            )
            .check(
                vec!["1", "2"],
                user_query()
                    .filter(EntityFilter::LessThan("name".to_owned(), "Kundi".into()))
                    .desc("name"),
            )
            .check(
                vec!["1"],
                user_query()
                    .filter(EntityFilter::LessThan("name".to_owned(), "ZZZ".into()))
                    .desc("name")
                    .first(1)
                    .skip(1),
            )
            .check(
                vec!["2"],
                user_query()
                    .filter(EntityFilter::And(vec![
                        EntityFilter::LessThan("name".to_owned(), "Cz".into()),
                        EntityFilter::Equal("name".to_owned(), "Cindini".into()),
                    ]))
                    .desc("name"),
            )
            .check(
                vec!["2"],
                user_query()
                    .filter(EntityFilter::EndsWith("name".to_owned(), "ini".into()))
                    .desc("name"),
            )
            .check(
                vec!["3", "1"],
                user_query()
                    .filter(EntityFilter::NotEndsWith("name".to_owned(), "ini".into()))
                    .desc("name"),
            )
            .check(
                vec!["1"],
                user_query()
                    .filter(EntityFilter::In("name".to_owned(), vec!["Johnton".into()]))
                    .desc("name"),
            )
            .check(
                vec!["1", "2"],
                user_query()
                    .filter(EntityFilter::NotIn(
                        "name".to_owned(),
                        vec!["Shaqueeena".into()],
                    ))
                    .desc("name"),
            );

        // Filter tests with float attributes
        QueryChecker::new(store.clone())
            .check(
                vec!["1"],
                user_query().filter(EntityFilter::Equal(
                    "weight".to_owned(),
                    Value::BigDecimal(184.4.into()),
                )),
            )
            .check(
                vec!["3", "2"],
                user_query()
                    .filter(EntityFilter::Not(
                        "weight".to_owned(),
                        Value::BigDecimal(184.4.into()),
                    ))
                    .desc("name"),
            )
            .check(
                vec!["1"],
                user_query().filter(EntityFilter::GreaterThan(
                    "weight".to_owned(),
                    Value::BigDecimal(160.0.into()),
                )),
            )
            .check(
                vec!["2", "3"],
                user_query()
                    .filter(EntityFilter::LessThan(
                        "weight".to_owned(),
                        Value::BigDecimal(160.0.into()),
                    ))
                    .asc("name"),
            )
            .check(
                vec!["3", "2"],
                user_query()
                    .filter(EntityFilter::LessThan(
                        "weight".to_owned(),
                        Value::BigDecimal(160.0.into()),
                    ))
                    .desc("name"),
            )
            .check(
                vec!["2"],
                user_query()
                    .filter(EntityFilter::LessThan(
                        "weight".to_owned(),
                        Value::BigDecimal(161.0.into()),
                    ))
                    .desc("name")
                    .first(1)
                    .skip(1),
            )
            .check(
                vec!["3", "1"],
                user_query()
                    .filter(EntityFilter::In(
                        "weight".to_owned(),
                        vec![
                            Value::BigDecimal(184.4.into()),
                            Value::BigDecimal(111.7.into()),
                        ],
                    ))
                    .desc("name")
                    .first(5),
            )
            .check(
                vec!["2"],
                user_query()
                    .filter(EntityFilter::NotIn(
                        "weight".to_owned(),
                        vec![
                            Value::BigDecimal(184.4.into()),
                            Value::BigDecimal(111.7.into()),
                        ],
                    ))
                    .desc("name")
                    .first(5),
            );
        // Filter tests with int attributes
        QueryChecker::new(store.clone())
            .check(
                vec!["1"],
                user_query()
                    .filter(EntityFilter::Equal("age".to_owned(), Value::Int(67_i32)))
                    .desc("name"),
            )
            .check(
                vec!["3", "2"],
                user_query()
                    .filter(EntityFilter::Not("age".to_owned(), Value::Int(67_i32)))
                    .desc("name"),
            )
            .check(
                vec!["1"],
                user_query().filter(EntityFilter::GreaterThan(
                    "age".to_owned(),
                    Value::Int(43_i32),
                )),
            )
            .check(
                vec!["2", "1"],
                user_query()
                    .filter(EntityFilter::GreaterOrEqual(
                        "age".to_owned(),
                        Value::Int(43_i32),
                    ))
                    .asc("name"),
            )
            .check(
                vec!["2", "3"],
                user_query()
                    .filter(EntityFilter::LessThan("age".to_owned(), Value::Int(50_i32)))
                    .asc("name"),
            )
            .check(
                vec!["2", "3"],
                user_query()
                    .filter(EntityFilter::LessOrEqual(
                        "age".to_owned(),
                        Value::Int(43_i32),
                    ))
                    .asc("name"),
            )
            .check(
                vec!["3", "2"],
                user_query()
                    .filter(EntityFilter::LessThan("age".to_owned(), Value::Int(50_i32)))
                    .desc("name"),
            )
            .check(
                vec!["2"],
                user_query()
                    .filter(EntityFilter::LessThan("age".to_owned(), Value::Int(67_i32)))
                    .desc("name")
                    .first(1)
                    .skip(1),
            )
            .check(
                vec!["1", "2"],
                user_query()
                    .filter(EntityFilter::In(
                        "age".to_owned(),
                        vec![Value::Int(67_i32), Value::Int(43_i32)],
                    ))
                    .desc("name")
                    .first(5),
            )
            .check(
                vec!["3"],
                user_query()
                    .filter(EntityFilter::NotIn(
                        "age".to_owned(),
                        vec![Value::Int(67_i32), Value::Int(43_i32)],
                    ))
                    .desc("name")
                    .first(5),
            );
        // Filter tests with bool attributes
        QueryChecker::new(store.clone())
            .check(
                vec!["2"],
                user_query()
                    .filter(EntityFilter::Equal("coffee".to_owned(), Value::Bool(true)))
                    .desc("name"),
            )
            .check(
                vec!["1", "3"],
                user_query()
                    .filter(EntityFilter::Not("coffee".to_owned(), Value::Bool(true)))
                    .asc("name"),
            )
            .check(
                vec!["2"],
                user_query()
                    .filter(EntityFilter::In(
                        "coffee".to_owned(),
                        vec![Value::Bool(true)],
                    ))
                    .desc("name")
                    .first(5),
            )
            .check(
                vec!["3", "1"],
                user_query()
                    .filter(EntityFilter::NotIn(
                        "coffee".to_owned(),
                        vec![Value::Bool(true)],
                    ))
                    .desc("name")
                    .first(5),
            );
        // Misc filter tests
        QueryChecker::new(store)
            .check(
                vec!["1"],
                user_query()
                    .filter(EntityFilter::Equal(
                        "bin_name".to_owned(),
                        Value::Bytes("Johnton".as_bytes().into()),
                    ))
                    .desc("name"),
            )
            .check(
                vec!["3", "1"],
                user_query()
                    .filter(EntityFilter::Equal(
                        "favorite_color".to_owned(),
                        Value::Null,
                    ))
                    .desc("name"),
            )
            .check(
                vec!["3", "1"],
                user_query()
                    .filter(EntityFilter::Equal(
                        "favorite_color".to_owned(),
                        Value::Null,
                    ))
                    .desc("name"),
            )
            .check(
                vec!["2"],
                user_query()
                    .filter(EntityFilter::Not("favorite_color".to_owned(), Value::Null))
                    .desc("name"),
            )
            .check(
                vec!["2"],
                user_query()
                    .filter(EntityFilter::NotIn(
                        "favorite_color".to_owned(),
                        vec![Value::Null],
                    ))
                    .desc("name"),
            )
            .check(vec!["3", "2", "1"], user_query().asc("weight"))
            .check(vec!["1", "2", "3"], user_query().desc("weight"))
            .check(vec!["1", "2", "3"], user_query().asc("id"))
            .check(vec!["3", "2", "1"], user_query().desc("id"))
            .check(vec!["3", "2", "1"], user_query().asc("age"))
            .check(vec!["1", "2", "3"], user_query().desc("age"))
            .check(vec!["2", "1", "3"], user_query().asc("name"))
            .check(vec!["3", "1", "2"], user_query().desc("name"))
            .check(
                vec!["1", "2"],
                user_query()
                    .filter(EntityFilter::And(vec![EntityFilter::Or(vec![
                        EntityFilter::Equal("id".to_owned(), Value::from("1")),
                        EntityFilter::Equal("id".to_owned(), Value::from("2")),
                    ])]))
                    .asc("id"),
            );
    });
}

fn make_entity_change(entity_type: &EntityType) -> EntityChange {
    EntityChange::Data {
        subgraph_id: TEST_SUBGRAPH_ID.clone(),
        entity_type: entity_type.to_string(),
    }
}

// Get as events until we've seen all the expected events or we time out waiting
async fn check_events(
    stream: StoreEventStream<impl Stream<Item = Arc<StoreEvent>, Error = ()> + Send>,
    expected: Vec<StoreEvent>,
) {
    fn as_set(events: Vec<Arc<StoreEvent>>) -> HashSet<EntityChange> {
        events.into_iter().fold(HashSet::new(), |mut set, event| {
            set.extend(event.changes.iter().cloned());
            set
        })
    }

    let expected = Mutex::new(as_set(expected.into_iter().map(Arc::new).collect()));
    // Capture extra changes here; this is only needed for debugging, really.
    // It's permissible that we get more changes than we expected because of
    // how store events group changes together
    let extra: Mutex<HashSet<EntityChange>> = Mutex::new(HashSet::new());
    // Get events from the store until we've either seen all the changes we
    // expected or we timed out waiting for them
    stream
        .take_while(|event| {
            let mut expected = expected.lock().unwrap();
            for change in &event.changes {
                if !expected.remove(change) {
                    extra.lock().unwrap().insert(change.clone());
                }
            }
            future::ok(!expected.is_empty())
        })
        .collect()
        .compat()
        .timeout(Duration::from_secs(3))
        .await
        .unwrap_or_else(|_| {
            panic!(
                "timed out waiting for events\n  still waiting for {:?}\n  got extra events {:?}",
                expected.lock().unwrap().clone(),
                extra.lock().unwrap().clone()
            )
        })
        .expect("something went wrong getting events");
    // Check again that we really got everything
    assert_eq!(HashSet::new(), expected.lock().unwrap().clone());
}

// Subscribe to store events
fn subscribe(
    subgraph: &DeploymentHash,
    entity_type: &EntityType,
) -> StoreEventStream<impl Stream<Item = Arc<StoreEvent>, Error = ()> + Send> {
    let subscription =
        SUBSCRIPTION_MANAGER.subscribe(FromIterator::from_iter([SubscriptionFilter::Entities(
            subgraph.clone(),
            entity_type.to_owned(),
        )]));

    StoreEventStream::new(subscription)
}

async fn check_basic_revert(
    store: Arc<DieselStore>,
    expected: StoreEvent,
    deployment: &DeploymentLocator,
    entity_type: &EntityType,
) {
    let this_query = user_query()
        .filter(EntityFilter::Equal(
            "name".to_owned(),
            Value::String("Shaqueeena".to_owned()),
        ))
        .desc("name");

    let subscription = subscribe(&deployment.hash, entity_type);
    let state = deployment_state(store.as_ref(), &deployment.hash).await;
    assert_eq!(&deployment.hash, &state.id);

    // Revert block 3
    revert_block(&store, deployment, &TEST_BLOCK_1_PTR).await;

    let returned_entities = store
        .subgraph_store()
        .find(this_query.clone())
        .expect("store.find operation failed");

    // There should be 1 user returned in results
    assert_eq!(1, returned_entities.len());

    // Check if the first user in the result vector has email "queensha@email.com"
    let returned_name = returned_entities[0].get("email");
    let test_value = Value::String("queensha@email.com".to_owned());
    assert!(returned_name.is_some());
    assert_eq!(&test_value, returned_name.unwrap());

    let state = deployment_state(store.as_ref(), &deployment.hash).await;
    assert_eq!(&deployment.hash, &state.id);

    check_events(subscription, vec![expected]).await
}

#[test]
fn revert_block_basic_user() {
    run_test(|store, _, deployment| async move {
        let expected = StoreEvent::new(vec![make_entity_change(&*USER_TYPE)]);

        let count = get_entity_count(store.clone(), &deployment.hash);
        check_basic_revert(store.clone(), expected, &deployment, &*USER_TYPE).await;
        assert_eq!(count, get_entity_count(store.clone(), &deployment.hash));
    })
}

#[test]
fn revert_block_with_delete() {
    run_test(|store, _, deployment| async move {
        let this_query = user_query()
            .filter(EntityFilter::Equal(
                "name".to_owned(),
                Value::String("Cindini".to_owned()),
            ))
            .desc("name");

        // Delete entity with id=2
        let del_key = USER_TYPE.parse_key("2").unwrap();

        // Process deletion
        transact_and_wait(
            &store.subgraph_store(),
            &deployment,
            TEST_BLOCK_3_PTR.clone(),
            vec![EntityOperation::Remove { key: del_key }],
        )
        .await
        .unwrap();

        let subscription = subscribe(&deployment.hash, &*USER_TYPE);

        // Revert deletion
        let count = get_entity_count(store.clone(), &deployment.hash);
        revert_block(&store, &deployment, &TEST_BLOCK_2_PTR).await;
        assert_eq!(count + 1, get_entity_count(store.clone(), &deployment.hash));

        // Query after revert
        let returned_entities = store
            .subgraph_store()
            .find(this_query.clone())
            .expect("store.find operation failed");

        // There should be 1 entity returned in results
        assert_eq!(1, returned_entities.len());

        // Check if "dinici@email.com" is in result set
        let returned_name = returned_entities[0].get("email");
        let test_value = Value::String("dinici@email.com".to_owned());
        assert!(returned_name.is_some());
        assert_eq!(&test_value, returned_name.unwrap());

        // Check that the subscription notified us of the changes
        let expected = StoreEvent::new(vec![make_entity_change(&*USER_TYPE)]);

        // The last event is the one for the reversion
        check_events(subscription, vec![expected]).await
    })
}

#[test]
fn revert_block_with_partial_update() {
    run_test(|store, writable, deployment| async move {
        let entity_key = USER_TYPE.parse_key("1").unwrap();
        let schema = writable.input_schema();

        let partial_entity = entity! { schema => id: "1", name: "Johnny Boy", email: Value::Null };

        let original_entity = writable.get(&entity_key).unwrap().expect("missing entity");

        // Set test entity; as the entity already exists an update should be performed
        transact_entity_operations(
            &store.subgraph_store(),
            &deployment,
            TEST_BLOCK_3_PTR.clone(),
            vec![EntityOperation::Set {
                key: entity_key.clone(),
                data: partial_entity.clone(),
            }],
        )
        .await
        .unwrap();

        let subscription = subscribe(&deployment.hash, &*USER_TYPE);

        // Perform revert operation, reversing the partial update
        let count = get_entity_count(store.clone(), &deployment.hash);
        revert_block(&store, &deployment, &TEST_BLOCK_2_PTR).await;
        assert_eq!(count, get_entity_count(store.clone(), &deployment.hash));

        // Obtain the reverted entity from the store
        let reverted_entity = writable.get(&entity_key).unwrap().expect("missing entity");

        // Verify that the entity has been returned to its original state
        assert_eq!(reverted_entity, original_entity);

        // Check that the subscription notified us of the changes
        let expected = StoreEvent::new(vec![make_entity_change(&*USER_TYPE)]);

        check_events(subscription, vec![expected]).await
    })
}

fn mock_data_source() -> graph_chain_ethereum::DataSource {
    graph_chain_ethereum::DataSource {
        kind: String::from("ethereum/contract"),
        name: String::from("example data source"),
        manifest_idx: 0,
        network: Some(String::from("mainnet")),
        address: Some(Address::from_str("0123123123012312312301231231230123123123").unwrap()),
        start_block: 0,
        end_block: None,
        mapping: Mapping {
            kind: String::from("ethereum/events"),
            api_version: Version::parse("0.1.0").unwrap(),
            language: String::from("wasm/assemblyscript"),
            entities: vec![],
            abis: vec![],
            event_handlers: vec![],
            call_handlers: vec![],
            block_handlers: vec![],
            link: Link {
                link: "link".to_owned(),
            },
            runtime: Arc::new(Vec::new()),
        },
        context: Default::default(),
        creation_block: None,
        contract_abi: Arc::new(mock_abi()),
    }
}

fn mock_abi() -> MappingABI {
    MappingABI {
        name: "mock_abi".to_string(),
        contract: Contract::load(
            r#"[
            {
                "inputs": [
                    {
                        "name": "a",
                        "type": "address"
                    }
                ],
                "type": "constructor"
            }
        ]"#
            .as_bytes(),
        )
        .unwrap(),
    }
}

#[test]
fn revert_block_with_dynamic_data_source_operations() {
    run_test(|store, writable, deployment| async move {
        let subgraph_store = store.subgraph_store();
        let schema = writable.input_schema();

        // Create operations to add a user
        let user_key = USER_TYPE.parse_key("1").unwrap();
        let partial_entity = entity! { schema => id: "1", name: "Johnny Boy", email: Value::Null };

        // Get the original user for comparisons
        let original_user = writable.get(&user_key).unwrap().expect("missing entity");

        // Create operations to add a dynamic data source
        let mut data_source = mock_data_source();
        let manifest_idx_and_name = vec![(0, "example data source".to_string())];

        let ops = vec![EntityOperation::Set {
            key: user_key.clone(),
            data: partial_entity.clone(),
        }];

        // Add user and dynamic data source to the store
        data_source.creation_block = Some(TEST_BLOCK_3_PTR.number);
        transact_entities_and_dynamic_data_sources(
            &subgraph_store,
            deployment.clone(),
            TEST_BLOCK_3_PTR.clone(),
            vec![data_source.as_stored_dynamic_data_source()],
            ops,
            manifest_idx_and_name.clone(),
        )
        .await
        .unwrap();

        // Verify that the user is no longer the original
        assert_ne!(
            writable.get(&user_key).unwrap().expect("missing entity"),
            original_user
        );

        // Verify that the dynamic data source exists afterwards
        let loaded_dds = writable
            .load_dynamic_data_sources(manifest_idx_and_name.clone())
            .await
            .unwrap();
        assert_eq!(1, loaded_dds.len());
        assert_eq!(
            data_source.address.unwrap().0,
            **loaded_dds[0].param.as_ref().unwrap()
        );

        let subscription = subscribe(&deployment.hash, &*USER_TYPE);

        // Revert block that added the user and the dynamic data source
        revert_block(&store, &deployment, &TEST_BLOCK_2_PTR).await;

        // Verify that the user is the original again
        assert_eq!(
            writable.get(&user_key).unwrap().expect("missing entity"),
            original_user
        );

        // Verify that the dynamic data source is gone after the reversion
        let loaded_dds = writable
            .load_dynamic_data_sources(manifest_idx_and_name)
            .await
            .unwrap();
        assert_eq!(0, loaded_dds.len());

        // Verify that the right change events were emitted for the reversion
        let expected_events = vec![StoreEvent {
            tag: 3,
            changes: HashSet::from_iter(
                vec![EntityChange::Data {
                    subgraph_id: DeploymentHash::new("testsubgraph").unwrap(),
                    entity_type: USER_TYPE.to_string(),
                }]
                .into_iter(),
            ),
        }];
        check_events(subscription, expected_events).await
    })
}

#[test]
fn entity_changes_are_fired_and_forwarded_to_subscriptions() {
    run_test(|store, _, _| async move {
        let subgraph_id = DeploymentHash::new("EntityChangeTestSubgraph").unwrap();
        let schema =
            InputSchema::parse(USER_GQL, subgraph_id.clone()).expect("Failed to parse user schema");
        let manifest = SubgraphManifest::<graph_chain_ethereum::Chain> {
            id: subgraph_id.clone(),
            spec_version: Version::new(1, 0, 0),
            features: Default::default(),
            description: None,
            repository: None,
            schema: schema.clone(),
            data_sources: vec![],
            graft: None,
            templates: vec![],
            chain: PhantomData,
            indexer_hints: None,
        };

        let deployment =
            DeploymentCreate::new(String::new(), &manifest, Some(TEST_BLOCK_0_PTR.clone()));
        let name = SubgraphName::new("test/entity-changes-are-fired").unwrap();
        let node_id = NodeId::new("test").unwrap();
        let deployment = store
            .subgraph_store()
            .create_subgraph_deployment(
                name,
                &schema,
                deployment,
                node_id,
                NETWORK_NAME.to_string(),
                SubgraphVersionSwitchingMode::Instant,
            )
            .unwrap();

        let subscription = subscribe(&subgraph_id, &*USER_TYPE);

        // Add two entities to the store
        let added_entities = vec![
            (
                "1".to_owned(),
                entity! { schema => id: "1", name: "Johnny Boy" },
            ),
            ("2".to_owned(), entity! { schema => id: "2", name: "Tessa" }),
        ];
        transact_and_wait(
            &store.subgraph_store(),
            &deployment,
            TEST_BLOCK_1_PTR.clone(),
            added_entities
                .iter()
                .map(|(id, data)| EntityOperation::Set {
                    key: USER_TYPE.parse_key(id.as_str()).unwrap(),
                    data: data.clone(),
                })
                .collect(),
        )
        .await
        .unwrap();

        // Update an entity in the store
        let updated_entity = entity! { schema => id: "1", name: "Johnny" };
        let update_op = EntityOperation::Set {
            key: USER_TYPE.parse_key("1").unwrap(),
            data: updated_entity.clone(),
        };

        // Delete an entity in the store
        let delete_op = EntityOperation::Remove {
            key: USER_TYPE.parse_key("2").unwrap(),
        };

        // Commit update & delete ops
        transact_and_wait(
            &store.subgraph_store(),
            &deployment,
            TEST_BLOCK_2_PTR.clone(),
            vec![update_op, delete_op],
        )
        .await
        .unwrap();

        // We're expecting two events to be written to the subscription stream
        let expected = vec![
            StoreEvent::new(vec![
                EntityChange::Data {
                    subgraph_id: subgraph_id.clone(),
                    entity_type: USER_TYPE.to_string(),
                },
                EntityChange::Data {
                    subgraph_id: subgraph_id.clone(),
                    entity_type: USER_TYPE.to_string(),
                },
            ]),
            StoreEvent::new(vec![
                EntityChange::Data {
                    subgraph_id: subgraph_id.clone(),
                    entity_type: USER_TYPE.to_string(),
                },
                EntityChange::Data {
                    subgraph_id: subgraph_id.clone(),
                    entity_type: USER_TYPE.to_string(),
                },
            ]),
        ];

        check_events(subscription, expected).await
    })
}

#[test]
fn throttle_subscription_delivers() {
    run_test(|store, _, deployment| async move {
        let subscription = subscribe(&deployment.hash, &*USER_TYPE)
            .throttle_while_syncing(
                &LOGGER,
                store
                    .clone()
                    .query_store(
                        QueryTarget::Deployment(deployment.hash.clone(), Default::default()),
                        true,
                    )
                    .await
                    .unwrap(),
                Duration::from_millis(500),
            )
            .await;

        let user4 = create_test_entity(
            "4",
            &*USER_TYPE,
            "Steve",
            "nieve@email.com",
            72_i32,
            120.7,
            false,
            None,
        );

        transact_entity_operations(
            &store.subgraph_store(),
            &deployment,
            TEST_BLOCK_3_PTR.clone(),
            vec![user4],
        )
        .await
        .unwrap();

        let expected = StoreEvent::new(vec![make_entity_change(&*USER_TYPE)]);

        check_events(subscription, vec![expected]).await
    })
}

#[test]
fn throttle_subscription_throttles() {
    run_test(|store, _, deployment| async move {
        // Throttle for a very long time (30s)
        let subscription = subscribe(&deployment.hash, &*USER_TYPE)
            .throttle_while_syncing(
                &LOGGER,
                store
                    .clone()
                    .query_store(
                        QueryTarget::Deployment(deployment.hash.clone(), Default::default()),
                        true,
                    )
                    .await
                    .unwrap(),
                Duration::from_secs(30),
            )
            .await;

        let user4 = create_test_entity(
            "4",
            &*USER_TYPE,
            "Steve",
            "nieve@email.com",
            72_i32,
            120.7,
            false,
            None,
        );

        transact_entity_operations(
            &store.subgraph_store(),
            &deployment,
            TEST_BLOCK_3_PTR.clone(),
            vec![user4],
        )
        .await
        .unwrap();

        // Make sure we time out waiting for the subscription
        let res = subscription
            .take(1)
            .collect()
            .compat()
            .timeout(Duration::from_millis(500))
            .await;
        assert!(res.is_err());
    })
}

#[test]
fn subgraph_schema_types_have_subgraph_id_directive() {
    run_test(|store, _, deployment| async move {
        let schema = store
            .subgraph_store()
            .api_schema(&deployment.hash, &Default::default())
            .expect("test subgraph should have a schema");
        for typedef in schema
            .definitions()
            .filter_map(|def| match def {
                s::Definition::TypeDefinition(typedef) => Some(typedef),
                _ => None,
            })
            .filter(|typedef| !typedef.is_introspection())
        {
            // Verify that all types have a @subgraphId directive on them
            let directive = match typedef {
                s::TypeDefinition::Object(t) => &t.directives,
                s::TypeDefinition::Interface(t) => &t.directives,
                s::TypeDefinition::Enum(t) => &t.directives,
                s::TypeDefinition::Scalar(t) => &t.directives,
                s::TypeDefinition::Union(t) => &t.directives,
                s::TypeDefinition::InputObject(t) => &t.directives,
            }
            .iter()
            .find(|directive| directive.name == "subgraphId")
            .expect("all subgraph schema types should have a @subgraphId directive");

            // Verify that all @subgraphId directives match the subgraph
            assert_eq!(
                directive.arguments,
                [(
                    String::from("id"),
                    s::Value::String(TEST_SUBGRAPH_ID_STRING.to_string())
                )]
            );
        }
    })
}

#[test]
fn handle_large_string_with_index() {
    const NAME: &str = "name";
    const ONE: &str = "large_string_one";
    const TWO: &str = "large_string_two";

    fn make_insert_op(
        id: &str,
        name: &str,
        schema: &InputSchema,
        block: BlockNumber,
    ) -> EntityModification {
        let data = entity! { schema => id: id, name: name };

        let key = USER_TYPE.parse_key(id).unwrap();

        EntityModification::insert(key, data, block)
    }

    run_test(|store, writable, deployment| async move {
        let schema = writable.input_schema();

        // We have to produce a massive string (1_000_000 chars) because
        // the repeated text compresses so well. This leads to an error
        // 'index row requires 11488 bytes, maximum size is 8191' if
        // used with a btree index without size limitation
        let long_text = "Quo usque tandem".repeat(62500);
        let other_text = long_text.clone() + "X";

        let metrics_registry = Arc::new(MetricsRegistry::mock());
        let stopwatch_metrics = StopwatchMetrics::new(
            Logger::root(slog::Discard, o!()),
            deployment.hash.clone(),
            "test",
            metrics_registry.clone(),
            "test_shard".to_string(),
        );

        let block = TEST_BLOCK_3_PTR.number;
        writable
            .transact_block_operations(
                TEST_BLOCK_3_PTR.clone(),
                FirehoseCursor::None,
                vec![
                    make_insert_op(ONE, &long_text, &schema, block),
                    make_insert_op(TWO, &other_text, &schema, block),
                ],
                &stopwatch_metrics,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                false,
            )
            .await
            .expect("Failed to insert large text");
        writable.flush().await.unwrap();

        let query = user_query()
            .first(5)
            .filter(EntityFilter::Equal(
                NAME.to_owned(),
                long_text.clone().into(),
            ))
            .asc(NAME);

        let ids: Vec<_> = store
            .subgraph_store()
            .find(query)
            .expect("Could not find entity")
            .iter()
            .map(|e| e.id())
            .collect();
        let exp = USER_TYPE.parse_ids(vec![ONE]).unwrap().as_ids();
        assert_eq!(exp, ids);

        // Make sure we check the full string and not just a prefix
        let mut prefix = long_text.clone();
        prefix.truncate(STRING_PREFIX_SIZE);
        let query = user_query()
            .first(5)
            .filter(EntityFilter::LessOrEqual(NAME.to_owned(), prefix.into()))
            .asc(NAME);

        let ids: Vec<_> = store
            .subgraph_store()
            .find(query)
            .expect("Could not find entity")
            .iter()
            .map(|e| e.id())
            .collect();

        // Users with name 'Cindini' and 'Johnton'
        let exp = USER_TYPE.parse_ids(vec!["2", "1"]).unwrap().as_ids();
        assert_eq!(exp, ids);
    })
}

#[test]
fn handle_large_bytea_with_index() {
    const NAME: &str = "bin_name";
    const ONE: &str = "large_string_one";
    const TWO: &str = "large_string_two";

    fn make_insert_op(
        id: &str,
        name: &[u8],
        schema: &InputSchema,
        block: BlockNumber,
    ) -> EntityModification {
        let data = entity! { schema => id: id, bin_name: scalar::Bytes::from(name) };

        let key = USER_TYPE.parse_key(id).unwrap();

        EntityModification::insert(key, data, block)
    }

    run_test(|store, writable, deployment| async move {
        let schema = writable.input_schema();

        // We have to produce a massive bytea (240_000 bytes) because the
        // repeated text compresses so well. This leads to an error 'index
        // row size 2784 exceeds btree version 4 maximum 2704' if used with
        // a btree index without size limitation
        let long_bytea = "Quo usque tandem".repeat(15000).into_bytes();
        let other_bytea = {
            let mut other_bytea = long_bytea.clone();
            other_bytea.push(b'X');
            scalar::Bytes::from(other_bytea.as_slice())
        };
        let long_bytea = scalar::Bytes::from(long_bytea.as_slice());

        let metrics_registry = Arc::new(MetricsRegistry::mock());
        let stopwatch_metrics = StopwatchMetrics::new(
            Logger::root(slog::Discard, o!()),
            deployment.hash.clone(),
            "test",
            metrics_registry.clone(),
            "test_shard".to_string(),
        );

        let block = TEST_BLOCK_3_PTR.number;
        writable
            .transact_block_operations(
                TEST_BLOCK_3_PTR.clone(),
                FirehoseCursor::None,
                vec![
                    make_insert_op(ONE, &long_bytea, &schema, block),
                    make_insert_op(TWO, &other_bytea, &schema, block),
                ],
                &stopwatch_metrics,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                false,
            )
            .await
            .expect("Failed to insert large text");
        writable.flush().await.unwrap();

        let query = user_query()
            .first(5)
            .filter(EntityFilter::Equal(
                NAME.to_owned(),
                long_bytea.clone().into(),
            ))
            .asc(NAME);

        let ids: Vec<_> = store
            .subgraph_store()
            .find(query)
            .expect("Could not find entity")
            .iter()
            .map(|e| e.id())
            .collect();

        let exp = USER_TYPE.parse_ids(vec![ONE]).unwrap().as_ids();
        assert_eq!(exp, ids);

        // Make sure we check the full string and not just a prefix
        let prefix = scalar::Bytes::from(&long_bytea.as_slice()[..64]);
        let query = user_query()
            .first(5)
            .filter(EntityFilter::LessOrEqual(NAME.to_owned(), prefix.into()))
            .asc(NAME);

        let ids: Vec<_> = store
            .subgraph_store()
            .find(query)
            .expect("Could not find entity")
            .iter()
            .map(|e| e.id())
            .collect();

        // Users with name 'Cindini' and 'Johnton'
        let exp = USER_TYPE.parse_ids(vec!["2", "1"]).unwrap().as_ids();
        assert_eq!(exp, ids);
    })
}

#[derive(Clone)]
struct WindowQuery(EntityQuery, Arc<DieselSubgraphStore>);

impl WindowQuery {
    fn new(store: &Arc<DieselStore>) -> Self {
        WindowQuery(
            user_query()
                .filter(EntityFilter::GreaterThan("age".into(), Value::from(0)))
                .first(10),
            store.subgraph_store(),
        )
        .default_window()
    }

    fn default_window(mut self) -> Self {
        let entity_types = match self.0.collection {
            EntityCollection::All(entity_types) => entity_types,
            EntityCollection::Window(_) => {
                unreachable!("we do not use this method with a windowed collection")
            }
        };
        let windows = entity_types
            .into_iter()
            .map(|(child_type, column_names)| {
                let attribute = WindowAttribute::Scalar("favorite_color".to_owned());
                let link = EntityLink::Direct(attribute, ChildMultiplicity::Many);
                let ids = child_type
                    .parse_ids(vec!["red", "green", "yellow", "blue"])
                    .unwrap();
                EntityWindow {
                    child_type,
                    ids,
                    link,
                    column_names,
                }
            })
            .collect();
        self.0.collection = EntityCollection::Window(windows);
        self
    }

    fn first(self, first: u32) -> Self {
        WindowQuery(self.0.first(first), self.1)
    }

    fn skip(self, skip: u32) -> Self {
        WindowQuery(self.0.skip(skip), self.1)
    }

    fn asc(self, attr: &str) -> Self {
        WindowQuery(
            self.0
                .order(EntityOrder::Ascending(attr.to_owned(), ValueType::String)),
            self.1,
        )
    }

    fn desc(self, attr: &str) -> Self {
        WindowQuery(
            self.0
                .order(EntityOrder::Descending(attr.to_owned(), ValueType::String)),
            self.1,
        )
    }

    fn unordered(self) -> Self {
        WindowQuery(self.0.order(EntityOrder::Unordered), self.1)
    }

    fn above(self, age: i32) -> Self {
        WindowQuery(
            self.0
                .filter(EntityFilter::GreaterThan("age".into(), Value::from(age))),
            self.1,
        )
    }

    fn against_color_and_age(self) -> Self {
        let mut query = self.0;
        query.collection = EntityCollection::All(vec![
            (USER_TYPE.clone(), AttributeNames::All),
            (PERSON_TYPE.clone(), AttributeNames::All),
        ]);
        WindowQuery(query, self.1).default_window()
    }

    fn expect(&self, mut expected_ids: Vec<&str>, qid: &str) {
        let query = self.0.clone();
        let store = &self.1;
        let unordered = matches!(query.order, EntityOrder::Unordered);
        let mut entity_ids = store
            .find(query)
            .expect("store.find failed to execute query")
            .into_iter()
            .map(|entity| match entity.get("id") {
                Some(Value::String(id)) => id.clone(),
                Some(_) => panic!("store.find returned entity with non-string ID attribute"),
                None => panic!("store.find returned entity with no ID attribute"),
            })
            .collect::<Vec<_>>();
        if unordered {
            entity_ids.sort();
            expected_ids.sort();
        }
        assert_eq!(expected_ids, entity_ids, "Failed query: {}", qid);
    }
}

#[test]
fn window() {
    fn make_color_and_age(
        entity_type: &EntityType,
        id: &str,
        color: &str,
        age: i32,
    ) -> EntityOperation {
        let entity = entity! { TEST_SUBGRAPH_SCHEMA => id: id, age: age, favorite_color: color };

        EntityOperation::Set {
            key: entity_type.parse_key(id).unwrap(),
            data: entity,
        }
    }

    fn make_user(id: &str, color: &str, age: i32) -> EntityOperation {
        make_color_and_age(&*USER_TYPE, id, color, age)
    }

    fn make_person(id: &str, color: &str, age: i32) -> EntityOperation {
        make_color_and_age(&*PERSON_TYPE, id, color, age)
    }

    let ops = vec![
        make_user("4", "green", 34),
        make_user("5", "green", 17),
        make_user("6", "green", 41),
        make_user("7", "red", 25),
        make_user("8", "red", 45),
        make_user("9", "yellow", 37),
        make_user("10", "blue", 27),
        make_user("11", "blue", 19),
        make_person("p1", "green", 12),
        make_person("p2", "red", 15),
    ];

    run_test(|store, _, deployment| async move {
        transact_and_wait(
            &store.subgraph_store(),
            &deployment,
            TEST_BLOCK_3_PTR.clone(),
            ops,
        )
        .await
        .expect("Failed to create test users");

        // Get the first 2 entries in each 'color group'
        WindowQuery::new(&store)
            .first(2)
            .expect(vec!["10", "11", "4", "5", "2", "7", "9"], "q1");

        WindowQuery::new(&store)
            .first(1)
            .expect(vec!["10", "4", "2", "9"], "q2");

        WindowQuery::new(&store)
            .first(1)
            .skip(1)
            .expect(vec!["11", "5", "7"], "q3");

        WindowQuery::new(&store)
            .first(1)
            .skip(1)
            .desc("id")
            .expect(vec!["10", "5", "7"], "q4");

        WindowQuery::new(&store)
            .first(1)
            .skip(1)
            .desc("favorite_color")
            .expect(vec!["10", "5", "7"], "q5");

        WindowQuery::new(&store)
            .first(1)
            .skip(1)
            .desc("favorite_color")
            .above(25)
            .expect(vec!["4", "2"], "q6");

        // Check queries for interfaces
        WindowQuery::new(&store)
            .first(1)
            .skip(1)
            .desc("favorite_color")
            .above(12)
            .against_color_and_age()
            .expect(vec!["10", "5", "8"], "q7");

        WindowQuery::new(&store)
            .first(1)
            .asc("age")
            .above(12)
            .against_color_and_age()
            .expect(vec!["11", "5", "p2", "9"], "q8");

        WindowQuery::new(&store)
            .unordered()
            .above(12)
            .against_color_and_age()
            .expect(
                vec!["10", "11", "2", "4", "5", "6", "7", "8", "9", "p2"],
                "q9",
            );
    });
}

#[test]
fn find_at_block() {
    fn shaqueeena_at_block(block: BlockNumber, email: &'static str) {
        run_test(move |store, _, _| async move {
            let mut query = user_query()
                .filter(EntityFilter::Equal("name".to_owned(), "Shaqueeena".into()))
                .desc("name");
            query.block = block;

            let entities = store
                .subgraph_store()
                .find(query)
                .expect("store.find failed to execute query");

            assert_eq!(1, entities.len());
            let entity = entities.first().unwrap();
            assert_eq!(Some(&Value::from(email)), entity.get("email"));
        })
    }

    shaqueeena_at_block(1, "queensha@email.com");
    shaqueeena_at_block(2, "teeko@email.com");
    shaqueeena_at_block(7000, "teeko@email.com");
}

#[test]
fn cleanup_cached_blocks() {
    if store_is_sharded() {
        // When the store is sharded, the setup in main.rs makes sure we
        // don't ever try to clean up cached blocks
        println!("store is sharded, skipping test");
        return;
    }

    run_test(|store, _, _| async move {
        use block_store::*;
        // The test subgraph is at block 2. Since we don't ever delete
        // the genesis block, the only block eligible for cleanup is BLOCK_ONE
        // and the first retained block is block 2.
        block_store::set_chain(
            vec![&*GENESIS_BLOCK, &*BLOCK_ONE, &*BLOCK_TWO, &*BLOCK_THREE],
            NETWORK_NAME,
        )
        .await;
        let chain_store = store
            .block_store()
            .chain_store(NETWORK_NAME)
            .expect("fake chain store");
        let cleaned = chain_store
            .cleanup_cached_blocks(10)
            .expect("cleanup succeeds");
        assert_eq!(Some((2, 1)), cleaned);
    })
}

#[test]
/// checks if retrieving the timestamp from the data blob works.
/// on ethereum, the block has timestamp as U256 so it will always have a value
fn parse_timestamp() {
    const EXPECTED_TS: u64 = 1657712166;

    run_test(|store, _, _| async move {
        use block_store::*;
        // The test subgraph is at block 2. Since we don't ever delete
        // the genesis block, the only block eligible for cleanup is BLOCK_ONE
        // and the first retained block is block 2.
        block_store::set_chain(
            vec![
                &*GENESIS_BLOCK,
                &*BLOCK_ONE,
                &*BLOCK_TWO,
                &*BLOCK_THREE_TIMESTAMP,
            ],
            NETWORK_NAME,
        )
        .await;
        let chain_store = store
            .block_store()
            .chain_store(NETWORK_NAME)
            .expect("fake chain store");

        let (_network, number, timestamp) = chain_store
            .block_number(&BLOCK_THREE_TIMESTAMP.block_hash())
            .await
            .expect("block_number to return correct number and timestamp")
            .unwrap();
        assert_eq!(number, 3);
        assert_eq!(timestamp.unwrap(), EXPECTED_TS);
    })
}

#[test]
fn parse_timestamp_firehose() {
    const EXPECTED_TS: u64 = 1657712166;

    run_test(|store, _, _| async move {
        use block_store::*;
        // The test subgraph is at block 2. Since we don't ever delete
        // the genesis block, the only block eligible for cleanup is BLOCK_ONE
        // and the first retained block is block 2.
        block_store::set_chain(
            vec![
                &*GENESIS_BLOCK,
                &*BLOCK_ONE,
                &*BLOCK_TWO,
                &*BLOCK_THREE_TIMESTAMP_FIREHOSE,
            ],
            NETWORK_NAME,
        )
        .await;
        let chain_store = store
            .block_store()
            .chain_store(NETWORK_NAME)
            .expect("fake chain store");

        let (_network, number, timestamp) = chain_store
            .block_number(&BLOCK_THREE_TIMESTAMP_FIREHOSE.block_hash())
            .await
            .expect("block_number to return correct number and timestamp")
            .unwrap();
        assert_eq!(number, 3);
        assert_eq!(timestamp.unwrap(), EXPECTED_TS);
    })
}

#[test]
/// checks if retrieving the timestamp from the data blob works.
/// on ethereum, the block has timestamp as U256 so it will always have a value
fn parse_null_timestamp() {
    run_test(|store, _, _| async move {
        use block_store::*;
        // The test subgraph is at block 2. Since we don't ever delete
        // the genesis block, the only block eligible for cleanup is BLOCK_ONE
        // and the first retained block is block 2.
        block_store::set_chain(
            vec![
                &*GENESIS_BLOCK,
                &*BLOCK_ONE,
                &*BLOCK_TWO,
                &*BLOCK_THREE_NO_TIMESTAMP,
            ],
            NETWORK_NAME,
        )
        .await;
        let chain_store = store
            .block_store()
            .chain_store(NETWORK_NAME)
            .expect("fake chain store");

        let (_network, number, timestamp) = chain_store
            .block_number(&BLOCK_THREE_NO_TIMESTAMP.block_hash())
            .await
            .expect("block_number to return correct number and timestamp")
            .unwrap();
        assert_eq!(number, 3);
        assert_eq!(true, timestamp.is_none());
    })
}
#[test]
fn reorg_tracking() {
    async fn update_john(
        store: &Arc<DieselSubgraphStore>,
        deployment: &DeploymentLocator,
        age: i32,
        block: &BlockPtr,
    ) {
        let test_entity_1 = create_test_entity(
            "1",
            &*USER_TYPE,
            "Johnton",
            "tonofjohn@email.com",
            age,
            184.4,
            false,
            None,
        );
        transact_and_wait(store, deployment, block.clone(), vec![test_entity_1])
            .await
            .unwrap();
    }

    macro_rules! check_state {
        ($store:expr,
         $reorg_count: expr,
         $max_reorg_depth:expr,
         $latest_ethereum_block_number:expr) => {
            let subgraph_id = TEST_SUBGRAPH_ID.to_owned();
            let state = deployment_state($store.as_ref(), &subgraph_id).await;
            assert_eq!(&subgraph_id, &state.id, "subgraph_id");
            assert_eq!($reorg_count, state.reorg_count, "reorg_count");
            assert_eq!($max_reorg_depth, state.max_reorg_depth, "max_reorg_depth");
            assert_eq!(
                $latest_ethereum_block_number, state.latest_block.number,
                "latest_ethereum_block_number"
            );
        };
    }

    // Check that reorg_count, max_reorg_depth, and latest_ethereum_block_number
    // are reported correctly in DeploymentState
    run_test(|store, _, deployment| async move {
        let subgraph_store = store.subgraph_store();

        check_state!(store, 0, 0, 2);

        // Jump to block 4
        transact_and_wait(
            &subgraph_store,
            &deployment,
            TEST_BLOCK_4_PTR.clone(),
            vec![],
        )
        .await
        .unwrap();
        check_state!(store, 0, 0, 4);

        // Back to block 3
        revert_block(&store, &deployment, &TEST_BLOCK_3_PTR).await;
        check_state!(store, 1, 1, 3);

        // Back to block 2
        revert_block(&store, &deployment, &TEST_BLOCK_2_PTR).await;
        check_state!(store, 2, 2, 2);

        // Forward to block 3
        update_john(&subgraph_store, &deployment, 70, &TEST_BLOCK_3_PTR).await;
        check_state!(store, 2, 2, 3);

        // Forward to block 4
        update_john(&subgraph_store, &deployment, 71, &TEST_BLOCK_4_PTR).await;
        check_state!(store, 2, 2, 4);

        // Forward to block 5
        update_john(&subgraph_store, &deployment, 72, &TEST_BLOCK_5_PTR).await;
        check_state!(store, 2, 2, 5);

        // Revert all the way back to block 2
        revert_block(&store, &deployment, &TEST_BLOCK_4_PTR).await;
        check_state!(store, 3, 2, 4);

        revert_block(&store, &deployment, &TEST_BLOCK_3_PTR).await;
        check_state!(store, 4, 2, 3);

        revert_block(&store, &deployment, &TEST_BLOCK_2_PTR).await;
        check_state!(store, 5, 3, 2);
    })
}
