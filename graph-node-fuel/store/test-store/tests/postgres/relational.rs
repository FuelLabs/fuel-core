//! Test mapping of GraphQL schema to a relational schema
use diesel::connection::SimpleConnection as _;
use diesel::pg::PgConnection;
use graph::data::store::{scalar, Id};
use graph::entity;
use graph::prelude::{
    o, slog, tokio, web3::types::H256, DeploymentHash, Entity, EntityCollection, EntityFilter,
    EntityOrder, EntityQuery, Logger, StopwatchMetrics, Value, ValueType, BLOCK_NUMBER_MAX,
};
use graph::prelude::{BlockNumber, MetricsRegistry};
use graph::schema::{EntityKey, EntityType, InputSchema};
use graph_store_postgres::layout_for_tests::set_account_like;
use graph_store_postgres::layout_for_tests::LayoutCache;
use graph_store_postgres::layout_for_tests::SqlName;
use hex_literal::hex;
use lazy_static::lazy_static;
use std::collections::BTreeSet;
use std::panic;
use std::str::FromStr;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

use graph::{
    components::store::AttributeNames,
    data::store::scalar::{BigDecimal, BigInt, Bytes},
};
use graph_store_postgres::{
    layout_for_tests::make_dummy_site,
    layout_for_tests::{Layout, Namespace, STRING_PREFIX_SIZE},
};

use test_store::*;

use crate::postgres::relational_bytes::{row_group_delete, row_group_insert, row_group_update};

const THINGS_GQL: &str = r#"
    type _Schema_ @fulltext(
        name: "userSearch"
        language: en
        algorithm: rank
        include: [
            {
                entity: "User",
                fields: [
                    { name: "name"},
                    { name: "email"},
                ]
            }
        ]
    ) @fulltext(
        name: "userSearch2"
        language: en
        algorithm: rank
        include: [
            {
                entity: "User",
                fields: [
                    { name: "name"},
                    { name: "email"},
                ]
            }
        ]
    ) @fulltext(
        name: "nullableStringsSearch"
        language: en
        algorithm: rank
        include: [
            {
                entity: "NullableStrings",
                fields: [
                    { name: "name"},
                    { name: "description"},
                    { name: "test"},
                ]
            }
        ]
    )

    type Thing @entity {
        id: ID!
        bigThing: Thing!
    }

    enum Color { yellow, red, BLUE }

    type Scalar @entity {
        id: ID,
        bool: Boolean,
        int: Int,
        bigDecimal: BigDecimal,
        bigDecimalArray: [BigDecimal!]!
        string: String,
        strings: [String!],
        bytes: Bytes,
        byteArray: [Bytes!],
        bigInt: BigInt,
        bigIntArray: [BigInt!]!
        color: Color,
        int8: Int8,
    }

    interface Pet {
        id: ID!,
        name: String!
    }

    type Cat implements Pet @entity {
        id: ID!,
        name: String!
    }

    type Dog implements Pet @entity {
        id: ID!,
        name: String!
    }

    type Ferret implements Pet @entity {
        id: ID!,
        name: String!
    }

    type Mink @entity(immutable: true) {
        id: ID!,
        order: Int,
    }

    type User @entity {
        id: ID!,
        name: String!,
        bin_name: Bytes!,
        email: String!,
        age: Int!,
        visits: Int8!
        seconds_age: BigInt!,
        weight: BigDecimal!,
        coffee: Boolean!,
        favorite_color: Color,
        drinks: [String!]
    }

    type NullableStrings @entity {
        id: ID!,
        name: String,
        description: String,
        test: String
    }

    interface BytePet {
        id: Bytes!,
        name: String!
    }

    type ByteCat implements BytePet @entity {
        id: Bytes!,
        name: String!
    }

    type ByteDog implements BytePet @entity {
        id: Bytes!,
        name: String!
    }

    type ByteFerret implements BytePet @entity {
        id: Bytes!,
        name: String!
    }
"#;

lazy_static! {
    static ref THINGS_SUBGRAPH_ID: DeploymentHash = DeploymentHash::new("things").unwrap();
    static ref THINGS_SCHEMA: InputSchema =
        InputSchema::parse(THINGS_GQL, THINGS_SUBGRAPH_ID.clone()).expect("failed to parse schema");
    static ref NAMESPACE: Namespace = Namespace::new("sgd0815".to_string()).unwrap();
    static ref LARGE_INT: BigInt = BigInt::from(std::i64::MAX).pow(17).unwrap();
    static ref LARGE_DECIMAL: BigDecimal =
        BigDecimal::from(1) / BigDecimal::new(LARGE_INT.clone(), 1);
    static ref BYTES_VALUE: H256 = H256::from(hex!(
        "e8b3b02b936c4a4a331ac691ac9a86e197fb7731f14e3108602c87d4dac55160"
    ));
    static ref BYTES_VALUE2: H256 = H256::from(hex!(
        "b98fb783b49de5652097a989414c767824dff7e7fd765a63b493772511db81c1"
    ));
    static ref BYTES_VALUE3: H256 = H256::from(hex!(
        "977c084229c72a0fa377cae304eda9099b6a2cb5d83b25cdf0f0969b69874255"
    ));
    static ref SCALAR_ENTITY: Entity = {
        let decimal = (*LARGE_DECIMAL).clone();
        let big_int = (*LARGE_INT).clone();
        entity! { THINGS_SCHEMA =>
            id: "one",
            bool: true,
            int: std::i32::MAX,
            int8: std::i64::MAX,
            bigDecimal: decimal.clone(),
            bigDecimalArray: vec![decimal.clone(), (decimal + 1.into())],
            string: "scalar",
            strings: vec!["left", "right", "middle"],
            bytes: *BYTES_VALUE,
            byteArray: vec![*BYTES_VALUE, *BYTES_VALUE2, *BYTES_VALUE3],
            bigInt: big_int.clone(),
            bigIntArray: vec![big_int.clone(), (big_int + 1.into())],
            color: "yellow",
        }
    };
    static ref EMPTY_NULLABLESTRINGS_ENTITY: Entity = {
        entity! { THINGS_SCHEMA =>
            id: "one",
        }
    };
    static ref SCALAR_TYPE: EntityType = THINGS_SCHEMA.entity_type("Scalar").unwrap();
    static ref USER_TYPE: EntityType = THINGS_SCHEMA.entity_type("User").unwrap();
    static ref DOG_TYPE: EntityType = THINGS_SCHEMA.entity_type("Dog").unwrap();
    static ref CAT_TYPE: EntityType = THINGS_SCHEMA.entity_type("Cat").unwrap();
    static ref FERRET_TYPE: EntityType = THINGS_SCHEMA.entity_type("Ferret").unwrap();
    static ref MINK_TYPE: EntityType = THINGS_SCHEMA.entity_type("Mink").unwrap();
    static ref CHAIR_TYPE: EntityType = THINGS_SCHEMA.entity_type("Chair").unwrap();
    static ref NULLABLE_STRINGS_TYPE: EntityType =
        THINGS_SCHEMA.entity_type("NullableStrings").unwrap();
    static ref MOCK_STOPWATCH: StopwatchMetrics = StopwatchMetrics::new(
        Logger::root(slog::Discard, o!()),
        THINGS_SUBGRAPH_ID.clone(),
        "test",
        Arc::new(MetricsRegistry::mock()),
        "test_shard".to_string()
    );
}

/// Removes test data from the database behind the store.
fn remove_schema(conn: &PgConnection) {
    let query = format!("drop schema if exists {} cascade", NAMESPACE.as_str());
    conn.batch_execute(&query)
        .expect("Failed to drop test schema");
}

fn insert_entity_at(
    conn: &PgConnection,
    layout: &Layout,
    entity_type: &EntityType,
    mut entities: Vec<Entity>,
    block: BlockNumber,
) {
    let entities_with_keys_owned = entities
        .drain(..)
        .map(|entity| {
            let key = entity_type.key(entity.id());
            (key, entity)
        })
        .collect::<Vec<(EntityKey, Entity)>>();
    let entities_with_keys: Vec<_> = entities_with_keys_owned
        .iter()
        .map(|(key, entity)| (key, entity))
        .collect();
    let errmsg = format!(
        "Failed to insert entities {}[{:?}]",
        entity_type, entities_with_keys
    );
    let group = row_group_insert(&entity_type, block, entities_with_keys_owned.clone());
    layout.insert(conn, &group, &MOCK_STOPWATCH).expect(&errmsg);
    assert_eq!(
        group.entity_count_change(),
        entities_with_keys_owned.len() as i32
    );
}

fn insert_entity(
    conn: &PgConnection,
    layout: &Layout,
    entity_type: &EntityType,
    entities: Vec<Entity>,
) {
    insert_entity_at(conn, layout, entity_type, entities, 0);
}

fn update_entity_at(
    conn: &PgConnection,
    layout: &Layout,
    entity_type: &EntityType,
    mut entities: Vec<Entity>,
    block: BlockNumber,
) {
    let entities_with_keys_owned: Vec<(EntityKey, Entity)> = entities
        .drain(..)
        .map(|entity| {
            let key = entity_type.key(entity.id());
            (key, entity)
        })
        .collect();
    let entities_with_keys: Vec<_> = entities_with_keys_owned
        .iter()
        .map(|(key, entity)| (key, entity))
        .collect();

    let errmsg = format!(
        "Failed to insert entities {}[{:?}]",
        entity_type, entities_with_keys
    );
    let group = row_group_update(&entity_type, block, entities_with_keys_owned.clone());
    let updated = layout.update(conn, &group, &MOCK_STOPWATCH).expect(&errmsg);
    assert_eq!(updated, entities_with_keys_owned.len());
}

fn insert_user_entity(
    conn: &PgConnection,
    layout: &Layout,
    id: &str,
    entity_type: &EntityType,
    name: &str,
    email: &str,
    age: i32,
    weight: f64,
    coffee: bool,
    favorite_color: Option<&str>,
    drinks: Option<Vec<&str>>,
    visits: i64,
    block: BlockNumber,
) {
    let user = make_user(
        &layout.input_schema,
        id,
        name,
        email,
        age,
        weight,
        coffee,
        favorite_color,
        drinks,
        visits,
    );

    insert_entity_at(conn, layout, entity_type, vec![user], block);
}

fn make_user(
    schema: &InputSchema,
    id: &str,
    name: &str,
    email: &str,
    age: i32,
    weight: f64,
    coffee: bool,
    favorite_color: Option<&str>,
    drinks: Option<Vec<&str>>,
    visits: i64,
) -> Entity {
    let favorite_color = favorite_color
        .map(|s| Value::String(s.to_owned()))
        .unwrap_or(Value::Null);
    let bin_name = Bytes::from_str(&hex::encode(name)).unwrap();
    let mut user = entity! { schema =>
        id: id,
        name: name,
        bin_name: bin_name,
        email: email,
        age: age,
        seconds_age: BigInt::from(age) * BigInt::from(31557600_u64),
        weight: BigDecimal::from(weight),
        coffee: coffee,
        favorite_color: favorite_color,
        visits: visits
    };
    if let Some(drinks) = drinks {
        user.insert("drinks", drinks.into()).unwrap();
    }
    user
}

fn insert_users(conn: &PgConnection, layout: &Layout) {
    insert_user_entity(
        conn,
        layout,
        "1",
        &*USER_TYPE,
        "Johnton",
        "tonofjohn@email.com",
        67_i32,
        184.4,
        false,
        Some("yellow"),
        None,
        60,
        0,
    );
    insert_user_entity(
        conn,
        layout,
        "2",
        &*USER_TYPE,
        "Cindini",
        "dinici@email.com",
        43_i32,
        159.1,
        true,
        Some("red"),
        Some(vec!["beer", "wine"]),
        50,
        0,
    );
    insert_user_entity(
        conn,
        layout,
        "3",
        &*USER_TYPE,
        "Shaqueeena",
        "teeko@email.com",
        28_i32,
        111.7,
        false,
        None,
        Some(vec!["coffee", "tea"]),
        22,
        0,
    );
}

fn update_user_entity(
    conn: &PgConnection,
    layout: &Layout,
    id: &str,
    entity_type: &EntityType,
    name: &str,
    email: &str,
    age: i32,
    weight: f64,
    coffee: bool,
    favorite_color: Option<&str>,
    drinks: Option<Vec<&str>>,
    visits: i64,
    block: BlockNumber,
) {
    let user = make_user(
        &layout.input_schema,
        id,
        name,
        email,
        age,
        weight,
        coffee,
        favorite_color,
        drinks,
        visits,
    );
    update_entity_at(conn, layout, entity_type, vec![user], block);
}

fn insert_pet(
    conn: &PgConnection,
    layout: &Layout,
    entity_type: &EntityType,
    id: &str,
    name: &str,
    block: BlockNumber,
) {
    let pet = entity! { layout.input_schema =>
        id: id,
        name: name
    };
    insert_entity_at(conn, layout, entity_type, vec![pet], block);
}

fn insert_pets(conn: &PgConnection, layout: &Layout) {
    insert_pet(conn, layout, &*DOG_TYPE, "pluto", "Pluto", 0);
    insert_pet(conn, layout, &*CAT_TYPE, "garfield", "Garfield", 0);
}

fn create_schema(conn: &PgConnection) -> Layout {
    let schema = InputSchema::parse(THINGS_GQL, THINGS_SUBGRAPH_ID.clone()).unwrap();
    let site = make_dummy_site(
        THINGS_SUBGRAPH_ID.clone(),
        NAMESPACE.clone(),
        NETWORK_NAME.to_string(),
    );
    let query = format!("create schema {}", NAMESPACE.as_str());
    conn.batch_execute(&query).unwrap();

    Layout::create_relational_schema(conn, Arc::new(site), &schema, BTreeSet::new())
        .expect("Failed to create relational schema")
}

fn scrub(entity: &Entity) -> Entity {
    let mut scrubbed = entity.clone();
    scrubbed.remove_null_fields();
    scrubbed
}

macro_rules! assert_entity_eq {
    ($left:expr, $right:expr) => {{
        let (left, right) = (&($left), &($right));
        let mut pass = true;

        for (key, left_value) in left.clone().sorted() {
            match right.get(&key) {
                None => {
                    pass = false;
                    println!("key '{}' missing from right", key);
                }
                Some(right_value) => {
                    if left_value != *right_value {
                        pass = false;
                        println!(
                            "values for '{}' differ:\n     left: {:?}\n    right: {:?}",
                            key, left_value, right_value
                        );
                    }
                }
            }
        }
        for (key, _) in right.clone().sorted() {
            if left.get(&key).is_none() {
                pass = false;
                println!("key '{}' missing from left", key);
            }
        }
        assert!(pass, "left and right entities are different");
    }};
}

/// Test harness for running database integration tests.
fn run_test<F>(test: F)
where
    F: FnOnce(&PgConnection, &Layout),
{
    run_test_with_conn(|conn| {
        // Reset state before starting
        remove_schema(conn);

        // Create the database schema
        let layout = create_schema(conn);

        // Run test
        test(conn, &layout);
    });
}

#[test]
fn find() {
    run_test(|conn, layout| {
        insert_entity(conn, layout, &*SCALAR_TYPE, vec![SCALAR_ENTITY.clone()]);

        // Happy path: find existing entity
        let entity = layout
            .find(
                conn,
                &SCALAR_TYPE.parse_key("one").unwrap(),
                BLOCK_NUMBER_MAX,
            )
            .expect("Failed to read Scalar[one]")
            .unwrap();
        assert_entity_eq!(scrub(&SCALAR_ENTITY), entity);

        // Find non-existing entity
        let entity = layout
            .find(
                conn,
                &SCALAR_TYPE.parse_key("noone").unwrap(),
                BLOCK_NUMBER_MAX,
            )
            .expect("Failed to read Scalar[noone]");
        assert!(entity.is_none());
    });
}

#[test]
fn insert_null_fulltext_fields() {
    run_test(|conn, layout| {
        insert_entity(
            conn,
            layout,
            &*NULLABLE_STRINGS_TYPE,
            vec![EMPTY_NULLABLESTRINGS_ENTITY.clone()],
        );

        // Find entity with null string values
        let entity = layout
            .find(
                conn,
                &NULLABLE_STRINGS_TYPE.parse_key("one").unwrap(),
                BLOCK_NUMBER_MAX,
            )
            .expect("Failed to read NullableStrings[one]")
            .unwrap();
        assert_entity_eq!(scrub(&EMPTY_NULLABLESTRINGS_ENTITY), entity);
    });
}

#[test]
fn update() {
    run_test(|conn, layout| {
        insert_entity(conn, layout, &*SCALAR_TYPE, vec![SCALAR_ENTITY.clone()]);

        // Update with overwrite
        let mut entity = SCALAR_ENTITY.clone();
        entity.set("string", "updated").unwrap();
        entity.remove("strings");
        entity.set("bool", Value::Null).unwrap();
        let key = SCALAR_TYPE.key(entity.id());

        let entity_type = layout.input_schema.entity_type("Scalar").unwrap();
        let entities = vec![(key, entity.clone())];
        let group = row_group_update(&entity_type, 0, entities);
        layout
            .update(conn, &group, &MOCK_STOPWATCH)
            .expect("Failed to update");

        let actual = layout
            .find(
                conn,
                &SCALAR_TYPE.parse_key("one").unwrap(),
                BLOCK_NUMBER_MAX,
            )
            .expect("Failed to read Scalar[one]")
            .unwrap();
        assert_entity_eq!(scrub(&entity), actual);
    });
}

#[test]
fn update_many() {
    run_test(|conn, layout| {
        let mut one = SCALAR_ENTITY.clone();
        let mut two = SCALAR_ENTITY.clone();
        two.set("id", "two").unwrap();
        let mut three = SCALAR_ENTITY.clone();
        three.set("id", "three").unwrap();
        insert_entity(
            conn,
            layout,
            &*SCALAR_TYPE,
            vec![one.clone(), two.clone(), three.clone()],
        );

        // confidence test: there should be 3 scalar entities in store right now
        assert_eq!(3, count_scalar_entities(conn, layout));

        // update with overwrite
        one.set("string", "updated").unwrap();
        one.remove("strings");

        two.set("string", "updated too").unwrap();
        two.set("bool", false).unwrap();

        three.set("string", "updated in a different way").unwrap();
        three.remove("strings");
        three.set("color", "red").unwrap();

        // generate keys
        let entity_type = layout.input_schema.entity_type("Scalar").unwrap();
        let keys: Vec<EntityKey> = ["one", "two", "three"]
            .iter()
            .map(|id| SCALAR_TYPE.parse_key(*id).unwrap())
            .collect();

        let entities_vec = vec![one, two, three];
        let entities: Vec<_> = keys.into_iter().zip(entities_vec.into_iter()).collect();
        let group = row_group_update(&entity_type, 0, entities);
        layout
            .update(conn, &group, &MOCK_STOPWATCH)
            .expect("Failed to update");

        // check updates took effect
        let updated: Vec<Entity> = ["one", "two", "three"]
            .iter()
            .map(|&id| {
                layout
                    .find(conn, &SCALAR_TYPE.parse_key(id).unwrap(), BLOCK_NUMBER_MAX)
                    .unwrap_or_else(|_| panic!("Failed to read Scalar[{}]", id))
                    .unwrap()
            })
            .collect();
        let new_one = &updated[0];
        let new_two = &updated[1];
        let new_three = &updated[2];

        // check they have the same id
        assert_eq!(new_one.get("id"), Some(&Value::String("one".to_string())));
        assert_eq!(new_two.get("id"), Some(&Value::String("two".to_string())));
        assert_eq!(
            new_three.get("id"),
            Some(&Value::String("three".to_string()))
        );

        // check their fields got updated as expected
        assert_eq!(
            new_one.get("string"),
            Some(&Value::String("updated".to_string()))
        );
        assert_eq!(new_one.get("strings"), None);
        assert_eq!(
            new_two.get("string"),
            Some(&Value::String("updated too".to_string()))
        );
        assert_eq!(new_two.get("bool"), Some(&Value::Bool(false)));
        assert_eq!(
            new_three.get("string"),
            Some(&Value::String("updated in a different way".to_string()))
        );
        assert_eq!(
            new_three.get("color"),
            Some(&Value::String("red".to_string()))
        );
    });
}

/// Test that we properly handle BigDecimal values with a negative scale.
#[test]
fn serialize_bigdecimal() {
    run_test(|conn, layout| {
        insert_entity(conn, layout, &*SCALAR_TYPE, vec![SCALAR_ENTITY.clone()]);

        // Update with overwrite
        let mut entity = SCALAR_ENTITY.clone();

        for d in &["50", "50.00", "5000", "0.5000", "0.050", "0.5", "0.05"] {
            let d = BigDecimal::from_str(d).unwrap();
            entity.set("bigDecimal", d).unwrap();

            let key = SCALAR_TYPE.key(entity.id());
            let entity_type = layout.input_schema.entity_type("Scalar").unwrap();
            let entities = vec![(key, entity.clone())];
            let group = row_group_update(&entity_type, 0, entities);
            layout
                .update(conn, &group, &MOCK_STOPWATCH)
                .expect("Failed to update");

            let actual = layout
                .find(
                    conn,
                    &SCALAR_TYPE.parse_key("one").unwrap(),
                    BLOCK_NUMBER_MAX,
                )
                .expect("Failed to read Scalar[one]")
                .unwrap();
            assert_entity_eq!(entity, actual);
        }
    });
}

fn count_scalar_entities(conn: &PgConnection, layout: &Layout) -> usize {
    let filter = EntityFilter::Or(vec![
        EntityFilter::Equal("bool".into(), true.into()),
        EntityFilter::Equal("bool".into(), false.into()),
    ]);
    let collection = EntityCollection::All(vec![(SCALAR_TYPE.to_owned(), AttributeNames::All)]);
    let mut query = EntityQuery::new(layout.site.deployment.clone(), BLOCK_NUMBER_MAX, collection)
        .filter(filter);
    query.range.first = None;
    layout
        .query::<Entity>(&LOGGER, conn, query)
        .map(|(entities, _)| entities)
        .expect("Count query failed")
        .len()
}

#[test]
fn delete() {
    run_test(|conn, layout| {
        insert_entity(conn, layout, &*SCALAR_TYPE, vec![SCALAR_ENTITY.clone()]);
        let mut two = SCALAR_ENTITY.clone();
        two.set("id", "two").unwrap();
        insert_entity(conn, layout, &*SCALAR_TYPE, vec![two]);

        // Delete where nothing is getting deleted
        let key = SCALAR_TYPE.parse_key("no such entity").unwrap();
        let entity_type = layout.input_schema.entity_type("Scalar").unwrap();
        let mut entity_keys = vec![key];
        let group = row_group_delete(&entity_type, 1, entity_keys.clone());
        let count = layout
            .delete(conn, &group, &MOCK_STOPWATCH)
            .expect("Failed to delete");
        assert_eq!(0, count);
        assert_eq!(2, count_scalar_entities(conn, layout));

        // Delete entity two
        entity_keys
            .get_mut(0)
            .map(|key| key.entity_id = SCALAR_TYPE.parse_id("two").unwrap())
            .expect("Failed to update key");

        let group = row_group_delete(&entity_type, 1, entity_keys);
        let count = layout
            .delete(conn, &group, &MOCK_STOPWATCH)
            .expect("Failed to delete");
        assert_eq!(1, count);
        assert_eq!(1, count_scalar_entities(conn, layout));
    });
}

#[test]
fn insert_many_and_delete_many() {
    run_test(|conn, layout| {
        let one = SCALAR_ENTITY.clone();
        let mut two = SCALAR_ENTITY.clone();
        two.set("id", "two").unwrap();
        let mut three = SCALAR_ENTITY.clone();
        three.set("id", "three").unwrap();
        insert_entity(conn, layout, &*SCALAR_TYPE, vec![one, two, three]);

        // confidence test: there should be 3 scalar entities in store right now
        assert_eq!(3, count_scalar_entities(conn, layout));

        // Delete entities with ids equal to "two" and "three"
        let entity_keys: Vec<_> = vec!["two", "three"]
            .into_iter()
            .map(|key| SCALAR_TYPE.parse_key(key).unwrap())
            .collect();
        let group = row_group_delete(&*SCALAR_TYPE, 1, entity_keys);
        let num_removed = layout
            .delete(conn, &group, &MOCK_STOPWATCH)
            .expect("Failed to delete");
        assert_eq!(2, num_removed);
        assert_eq!(1, count_scalar_entities(conn, layout));
    });
}

#[tokio::test]
async fn layout_cache() {
    // We need to use `block_on` to call the `create_test_subgraph` function which must be called
    // from a sync context, so we replicate what we do `spawn_module`.
    let runtime = tokio::runtime::Handle::current();
    std::thread::spawn(move || {
        run_test_with_conn(|conn| {
            let _runtime_guard = runtime.enter();

            let id = DeploymentHash::new("primaryLayoutCache").unwrap();
            let _loc = graph::block_on(create_test_subgraph(&id, THINGS_GQL));
            let site = Arc::new(primary_mirror().find_active_site(&id).unwrap().unwrap());
            let table_name = SqlName::verbatim("scalar".to_string());

            let cache = LayoutCache::new(Duration::from_millis(10));

            // Without an entry, account_like is false
            let layout = cache
                .get(&LOGGER, conn, site.clone())
                .expect("we can get the layout");
            let table = layout.table(&table_name).unwrap();
            assert_eq!(false, table.is_account_like);

            set_account_like(conn, site.as_ref(), &table_name, true)
                .expect("we can set 'scalar' to account-like");
            sleep(Duration::from_millis(50));

            // Flip account_like to true
            let layout = cache
                .get(&LOGGER, conn, site.clone())
                .expect("we can get the layout");
            let table = layout.table(&table_name).unwrap();
            assert_eq!(true, table.is_account_like);

            // Set it back to false
            set_account_like(conn, site.as_ref(), &table_name, false)
                .expect("we can set 'scalar' to account-like");
            sleep(Duration::from_millis(50));

            let layout = cache
                .get(&LOGGER, conn, site)
                .expect("we can get the layout");
            let table = layout.table(&table_name).unwrap();
            assert_eq!(false, table.is_account_like);
        })
    })
    .join()
    .unwrap();
}

#[test]
fn conflicting_entity() {
    // `id` is the id of an entity to create, `cat`, `dog`, and `ferret` are
    // the names of the types for which to check entity uniqueness
    fn check(conn: &PgConnection, layout: &Layout, id: Value, cat: &str, dog: &str, ferret: &str) {
        let conflicting = |types: Vec<&EntityType>| {
            let types = types.into_iter().cloned().collect();
            let id = Id::try_from(id.clone()).unwrap();
            layout.conflicting_entity(conn, &id, types)
        };

        let cat_type = layout.input_schema.entity_type(cat).unwrap();
        let dog_type = layout.input_schema.entity_type(dog).unwrap();
        let ferret_type = layout.input_schema.entity_type(ferret).unwrap();

        let fred = entity! { layout.input_schema => id: id.clone(), name: id.clone() };
        insert_entity(conn, layout, &cat_type, vec![fred]);

        // If we wanted to create Fred the dog, which is forbidden, we'd run this:
        let conflict = conflicting(vec![&cat_type, &ferret_type]).unwrap();
        assert_eq!(Some(cat.to_string()), conflict);

        // If we wanted to manipulate Fred the cat, which is ok, we'd run:
        let conflict = conflicting(vec![&dog_type, &ferret_type]).unwrap();
        assert_eq!(None, conflict);
    }

    run_test(|conn, layout| {
        let id = Value::String("fred".to_string());
        check(conn, layout, id, "Cat", "Dog", "Ferret");

        let id = Value::Bytes(scalar::Bytes::from_str("0xf1ed").unwrap());
        check(conn, layout, id, "ByteCat", "ByteDog", "ByteFerret");
    })
}

#[test]
fn revert_block() {
    fn check_fred(conn: &PgConnection, layout: &Layout) {
        let id = "fred";

        let set_fred = |name, block| {
            let fred = entity! { layout.input_schema =>
                id: id,
                name: name
            };
            if block == 0 {
                insert_entity_at(conn, layout, &*CAT_TYPE, vec![fred], block);
            } else {
                update_entity_at(conn, layout, &*CAT_TYPE, vec![fred], block);
            }
        };

        let assert_fred = |name: &str| {
            let fred = layout
                .find(conn, &CAT_TYPE.parse_key(id).unwrap(), BLOCK_NUMBER_MAX)
                .unwrap()
                .expect("there's a fred");
            assert_eq!(name, fred.get("name").unwrap().as_str().unwrap())
        };

        set_fred("zero", 0);
        set_fred("one", 1);
        set_fred("two", 2);
        set_fred("three", 3);

        layout.revert_block(conn, 3).unwrap();
        assert_fred("two");
        layout.revert_block(conn, 2).unwrap();
        assert_fred("one");

        set_fred("three", 3);
        assert_fred("three");
        layout.revert_block(conn, 3).unwrap();
        assert_fred("one");
    }

    fn check_marty(conn: &PgConnection, layout: &Layout) {
        let set_marties = |from, to| {
            for block in from..=to {
                let id = format!("marty-{}", block);
                let marty = entity! { layout.input_schema =>
                    id: id,
                    order: block,
                };
                insert_entity_at(conn, layout, &*MINK_TYPE, vec![marty], block);
            }
        };

        let assert_marties = |max_block, except: Vec<BlockNumber>| {
            let id = DeploymentHash::new("QmXW3qvxV7zXnwRntpj7yoK8HZVtaraZ67uMqaLRvXdxha").unwrap();
            let collection = EntityCollection::All(vec![(MINK_TYPE.clone(), AttributeNames::All)]);
            let filter = EntityFilter::StartsWith("id".to_string(), Value::from("marty"));
            let query = EntityQuery::new(id, BLOCK_NUMBER_MAX, collection)
                .filter(filter)
                .first(100)
                .order(EntityOrder::Ascending("order".to_string(), ValueType::Int));
            let marties: Vec<Entity> = layout
                .query(&LOGGER, conn, query)
                .map(|(entities, _)| entities)
                .expect("loading all marties works");

            let mut skipped = 0;
            for block in 0..=max_block {
                if except.contains(&block) {
                    skipped += 1;
                    continue;
                }
                let marty = &marties[block as usize - skipped];
                let id = format!("marty-{}", block);
                assert_eq!(id, marty.get("id").unwrap().as_str().unwrap());
                assert_eq!(block, marty.get("order").unwrap().as_int().unwrap())
            }
        };

        let assert_all_marties = |max_block| assert_marties(max_block, vec![]);

        set_marties(0, 4);
        assert_all_marties(4);

        layout.revert_block(conn, 3).unwrap();
        assert_all_marties(2);
        layout.revert_block(conn, 2).unwrap();
        assert_all_marties(1);

        set_marties(4, 4);
        // We don't have entries for 2 and 3 anymore
        assert_marties(4, vec![2, 3]);

        layout.revert_block(conn, 2).unwrap();
        assert_all_marties(1);
    }

    run_test(|conn, layout| {
        check_fred(conn, layout);
        check_marty(conn, layout);
    });
}

struct QueryChecker<'a> {
    conn: &'a PgConnection,
    layout: &'a Layout,
}

impl<'a> QueryChecker<'a> {
    fn new(conn: &'a PgConnection, layout: &'a Layout) -> Self {
        insert_users(conn, layout);
        update_user_entity(
            conn,
            layout,
            "1",
            &*USER_TYPE,
            "Jono",
            "achangedemail@email.com",
            67_i32,
            184.4,
            false,
            Some("yellow"),
            None,
            23,
            0,
        );
        insert_pets(conn, layout);

        Self { conn, layout }
    }

    fn check(self, expected_entity_ids: Vec<&'static str>, mut query: EntityQuery) -> Self {
        let q = query.clone();
        let unordered = matches!(query.order, EntityOrder::Unordered);
        query.block = BLOCK_NUMBER_MAX;
        let entities = self
            .layout
            .query::<Entity>(&LOGGER, self.conn, query)
            .expect("layout.query failed to execute query")
            .0;

        let mut entity_ids: Vec<_> = entities
            .into_iter()
            .map(|entity| match entity.get("id") {
                Some(Value::String(id)) => id.clone(),
                Some(_) => panic!("layout.query returned entity with non-string ID attribute"),
                None => panic!("layout.query returned entity with no ID attribute"),
            })
            .collect();

        let mut expected_entity_ids: Vec<String> =
            expected_entity_ids.into_iter().map(str::to_owned).collect();

        if unordered {
            entity_ids.sort();
            expected_entity_ids.sort();
        }

        assert_eq!(entity_ids, expected_entity_ids, "{:?}", q);
        self
    }
}

fn query(entity_types: &[&EntityType]) -> EntityQuery {
    EntityQuery::new(
        THINGS_SUBGRAPH_ID.clone(),
        BLOCK_NUMBER_MAX,
        EntityCollection::All(
            entity_types
                .into_iter()
                .map(|entity_type| ((*entity_type).clone(), AttributeNames::All))
                .collect(),
        ),
    )
}

fn user_query() -> EntityQuery {
    query(&vec![&*USER_TYPE])
}

trait EasyOrder {
    fn asc(self, attr: &str) -> Self;
    fn desc(self, attr: &str) -> Self;
    fn unordered(self) -> Self;
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

    fn unordered(self) -> Self {
        self.order(EntityOrder::Unordered)
    }
}

#[test]
#[should_panic(
    expected = "layout.query failed to execute query: FulltextQueryInvalidSyntax(\"syntax error in tsquery: \\\"Jono 'a\\\"\")"
)]
fn check_fulltext_search_syntax_error() {
    run_test(move |conn, layout| {
        QueryChecker::new(conn, layout).check(
            vec!["1"],
            user_query().filter(EntityFilter::Equal("userSearch".into(), "Jono 'a".into())),
        );
    });
}

#[test]
fn check_block_finds() {
    run_test(move |conn, layout| {
        let checker = QueryChecker::new(conn, layout);

        update_user_entity(
            conn,
            layout,
            "1",
            &*USER_TYPE,
            "Johnton",
            "tonofjohn@email.com",
            67_i32,
            184.4,
            false,
            Some("yellow"),
            None,
            55,
            1,
        );

        checker
            // Max block, we should get nothing
            .check(
                vec![],
                user_query().filter(EntityFilter::ChangeBlockGte(BLOCK_NUMBER_MAX)),
            )
            // Initial block, we should get here all data
            .check(
                vec!["1", "2", "3"],
                user_query().filter(EntityFilter::ChangeBlockGte(0)),
            )
            // Block with an update, we should have one only
            .check(
                vec!["1"],
                user_query().filter(EntityFilter::ChangeBlockGte(1)),
            );
    });
}

#[test]
fn check_find() {
    run_test(move |conn, layout| {
        // find with interfaces
        let types = vec![&*CAT_TYPE, &*DOG_TYPE];
        let checker = QueryChecker::new(conn, layout)
            .check(vec!["garfield", "pluto"], query(&types))
            .check(vec!["pluto", "garfield"], query(&types).desc("name"))
            .check(
                vec!["garfield"],
                query(&types)
                    .filter(EntityFilter::StartsWith("name".into(), Value::from("Gar")))
                    .desc("name"),
            )
            .check(vec!["pluto", "garfield"], query(&types).desc("id"))
            .check(vec!["garfield", "pluto"], query(&types).asc("id"))
            .check(vec!["garfield", "pluto"], query(&types).unordered());

        // fulltext
        let checker = checker
            .check(
                vec!["3"],
                user_query().filter(EntityFilter::Equal("userSearch".into(), "Shaq:*".into())),
            )
            .check(
                vec!["1"],
                user_query().filter(EntityFilter::Equal(
                    "userSearch".into(),
                    "Jono & achangedemail@email.com".into(),
                )),
            );
        // Test with a second fulltext search; we had a bug that caused only
        // one search index to be populated (see issue #4794)
        let checker = checker
            .check(
                vec!["3"],
                user_query().filter(EntityFilter::Equal("userSearch2".into(), "Shaq:*".into())),
            )
            .check(
                vec!["1"],
                user_query().filter(EntityFilter::Equal(
                    "userSearch2".into(),
                    "Jono & achangedemail@email.com".into(),
                )),
            );

        // list contains
        fn drinks_query(v: Vec<&str>) -> EntityQuery {
            let drinks: Option<Value> = Some(v.into());
            user_query().filter(EntityFilter::Contains("drinks".into(), drinks.into()))
        }

        let checker = checker
            .check(vec!["2"], drinks_query(vec!["beer"]))
            // Reverse of how we stored it
            .check(vec!["3"], drinks_query(vec!["tea", "coffee"]))
            .check(vec![], drinks_query(vec!["beer", "tea"]))
            .check(vec![], drinks_query(vec!["beer", "water"]))
            .check(vec![], drinks_query(vec!["beer", "wine", "water"]));

        // list not contains
        let checker = checker
            // User 3 do not have "beer" on its drinks list.
            .check(
                vec!["3"],
                user_query().filter(EntityFilter::NotContains(
                    "drinks".into(),
                    vec!["beer"].into(),
                )),
            )
            // Users 2 do not have "tea" on its drinks list.
            .check(
                vec!["2"],
                user_query().filter(EntityFilter::NotContains(
                    "drinks".into(),
                    vec!["tea"].into(),
                )),
            );

        // string attributes
        let checker = checker
            .check(
                vec!["2"],
                user_query().filter(EntityFilter::Contains("name".into(), "ind".into())),
            )
            .check(
                vec!["2"],
                user_query().filter(EntityFilter::Equal("name".to_owned(), "Cindini".into())),
            )
            // Test that we can order by id
            .check(
                vec!["2"],
                user_query()
                    .filter(EntityFilter::Equal("name".to_owned(), "Cindini".into()))
                    .desc("id"),
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
                    .filter(EntityFilter::In(
                        "name".to_owned(),
                        vec!["Jono".into(), "Nobody".into(), "Still nobody".into()],
                    ))
                    .desc("name"),
            )
            .check(
                vec![],
                user_query().filter(EntityFilter::In("name".to_owned(), vec![])),
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
        // float attributes
        let checker = checker
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

        // int 8 attributes
        let checker = checker.check(
            vec!["3"],
            user_query()
                .filter(EntityFilter::Equal("visits".to_owned(), Value::Int(22_i32)))
                .desc("name"),
        );

        // int attributes
        let checker = checker
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

        // bool attributes
        let checker = checker
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
        // misc tests
        let checker = checker
            .check(
                vec!["1"],
                user_query()
                    .filter(EntityFilter::Equal(
                        "bin_name".to_owned(),
                        Value::Bytes("Jono".as_bytes().into()),
                    ))
                    .desc("name"),
            )
            .check(
                vec!["3"],
                user_query()
                    .filter(EntityFilter::Equal(
                        "favorite_color".to_owned(),
                        Value::Null,
                    ))
                    .desc("name"),
            )
            .check(
                vec!["1", "2"],
                user_query()
                    .filter(EntityFilter::Not("favorite_color".to_owned(), Value::Null))
                    .desc("name"),
            )
            .check(
                vec!["1", "2"],
                user_query()
                    .filter(EntityFilter::NotIn(
                        "favorite_color".to_owned(),
                        vec![Value::Null],
                    ))
                    .desc("name"),
            )
            .check(
                vec!["1", "2"],
                user_query()
                    .filter(EntityFilter::NotIn(
                        "favorite_color".to_owned(),
                        vec!["red".into(), Value::Null],
                    ))
                    .desc("name"),
            )
            .check(vec!["3", "2", "1"], user_query().asc("weight"))
            .check(vec!["1", "2", "3"], user_query().desc("weight"))
            .check(vec!["1", "2", "3"], user_query().unordered())
            .check(vec!["1", "2", "3"], user_query().asc("id"))
            .check(vec!["3", "2", "1"], user_query().desc("id"))
            .check(vec!["1", "2", "3"], user_query().unordered())
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

        // enum attributes
        let checker = checker
            .check(
                vec!["2"],
                user_query()
                    .filter(EntityFilter::Equal(
                        "favorite_color".to_owned(),
                        "red".into(),
                    ))
                    .desc("name"),
            )
            .check(
                vec!["1"],
                user_query()
                    .filter(EntityFilter::Not("favorite_color".to_owned(), "red".into()))
                    .asc("name"),
            )
            .check(
                vec!["2"],
                user_query()
                    .filter(EntityFilter::In(
                        "favorite_color".to_owned(),
                        vec!["red".into()],
                    ))
                    .desc("name")
                    .first(5),
            )
            .check(
                vec!["1"],
                user_query()
                    .filter(EntityFilter::NotIn(
                        "favorite_color".to_owned(),
                        vec!["red".into()],
                    ))
                    .desc("name")
                    .first(5),
            );

        // empty and / or

        // It's somewhat arbitrary that we define empty 'or' and 'and' to
        // be 'true' and 'false'; it's mostly this way since that's what the
        // JSONB storage filters do

        checker
            // An empty 'or' is 'false'
            .check(
                vec![],
                user_query().filter(EntityFilter::And(vec![EntityFilter::Or(vec![])])),
            )
            // An empty 'and' is 'true'
            .check(
                vec!["1", "2", "3"],
                user_query().filter(EntityFilter::Or(vec![EntityFilter::And(vec![])])),
            );
    })
}

// We call our test strings aN so that
//   aN = "a" * (STRING_PREFIX_SIZE - 2 + N)
// chosen so that they straddle the boundary between strings that fit into
// the index, and strings that have only a prefix in the index
// Return (a1, a2, a2b, a3)
// Note that that is the order for these ids, though the
// underlying strings are in the order a1 < a2 < a3 < a2b
fn ferrets() -> (String, String, String, String) {
    (
        "a".repeat(STRING_PREFIX_SIZE - 1),
        "a".repeat(STRING_PREFIX_SIZE),
        format!("{}b", "a".repeat(STRING_PREFIX_SIZE)),
        "a".repeat(STRING_PREFIX_SIZE + 1),
    )
}

struct FilterChecker<'a> {
    conn: &'a PgConnection,
    layout: &'a Layout,
}

impl<'a> FilterChecker<'a> {
    fn new(conn: &'a PgConnection, layout: &'a Layout) -> Self {
        let (a1, a2, a2b, a3) = ferrets();
        insert_pet(conn, layout, &*FERRET_TYPE, "a1", &a1, 0);
        insert_pet(conn, layout, &*FERRET_TYPE, "a2", &a2, 0);
        insert_pet(conn, layout, &*FERRET_TYPE, "a2b", &a2b, 0);
        insert_pet(conn, layout, &*FERRET_TYPE, "a3", &a3, 0);

        Self { conn, layout }
    }

    fn check(&self, expected_entity_ids: Vec<&'static str>, filter: EntityFilter) -> &Self {
        let expected_entity_ids: Vec<String> =
            expected_entity_ids.into_iter().map(str::to_owned).collect();

        let query = query(&vec![&*FERRET_TYPE]).filter(filter).asc("id");

        let entities = self
            .layout
            .query::<Entity>(&LOGGER, self.conn, query)
            .expect("layout.query failed to execute query")
            .0;

        let entity_ids: Vec<_> = entities
            .into_iter()
            .map(|entity| match entity.get("id") {
                Some(Value::String(id)) => id.clone(),
                Some(_) => panic!("layout.query returned entity with non-string ID attribute"),
                None => panic!("layout.query returned entity with no ID attribute"),
            })
            .collect();

        assert_eq!(expected_entity_ids, entity_ids);
        self
    }
}

#[test]
fn check_filters() {
    let (a1, a2, a2b, a3) = ferrets();

    fn filter_eq(name: &str) -> EntityFilter {
        EntityFilter::Equal("name".to_owned(), name.into())
    }

    fn filter_block_gte(block: BlockNumber) -> EntityFilter {
        EntityFilter::ChangeBlockGte(block)
    }

    fn filter_not(name: &str) -> EntityFilter {
        EntityFilter::Not("name".to_owned(), name.into())
    }

    fn filter_lt(name: &str) -> EntityFilter {
        EntityFilter::LessThan("name".to_owned(), name.into())
    }

    fn filter_le(name: &str) -> EntityFilter {
        EntityFilter::LessOrEqual("name".to_owned(), name.into())
    }

    fn filter_gt(name: &str) -> EntityFilter {
        EntityFilter::GreaterThan("name".to_owned(), name.into())
    }

    fn filter_ge(name: &str) -> EntityFilter {
        EntityFilter::GreaterOrEqual("name".to_owned(), name.into())
    }

    fn filter_in(names: Vec<&str>) -> EntityFilter {
        EntityFilter::In(
            "name".to_owned(),
            names
                .into_iter()
                .map(|name| Value::from(name.to_owned()))
                .collect(),
        )
    }

    fn filter_not_in(names: Vec<&str>) -> EntityFilter {
        EntityFilter::NotIn(
            "name".to_owned(),
            names
                .into_iter()
                .map(|name| Value::from(name.to_owned()))
                .collect(),
        )
    }

    run_test(move |conn, layout| {
        let checker = FilterChecker::new(conn, layout);

        checker
            .check(vec!["a1"], filter_eq(&a1))
            .check(vec!["a2"], filter_eq(&a2))
            .check(vec!["a2b"], filter_eq(&a2b))
            .check(vec!["a3"], filter_eq(&a3));

        checker
            .check(vec!["a2", "a2b", "a3"], filter_not(&a1))
            .check(vec!["a1", "a2b", "a3"], filter_not(&a2))
            .check(vec!["a1", "a2", "a3"], filter_not(&a2b))
            .check(vec!["a1", "a2", "a2b"], filter_not(&a3));

        checker
            .check(vec![], filter_lt(&a1))
            .check(vec!["a1"], filter_lt(&a2))
            .check(vec!["a1", "a2", "a3"], filter_lt(&a2b))
            .check(vec!["a1", "a2"], filter_lt(&a3));

        checker
            .check(vec!["a1"], filter_le(&a1))
            .check(vec!["a1", "a2"], filter_le(&a2))
            .check(vec!["a1", "a2", "a2b", "a3"], filter_le(&a2b))
            .check(vec!["a1", "a2", "a3"], filter_le(&a3));

        checker
            .check(vec!["a2", "a2b", "a3"], filter_gt(&a1))
            .check(vec!["a2b", "a3"], filter_gt(&a2))
            .check(vec![], filter_gt(&a2b))
            .check(vec!["a2b"], filter_gt(&a3));

        checker
            .check(vec!["a1", "a2", "a2b", "a3"], filter_ge(&a1))
            .check(vec!["a2", "a2b", "a3"], filter_ge(&a2))
            .check(vec!["a2b"], filter_ge(&a2b))
            .check(vec!["a2b", "a3"], filter_ge(&a3));

        checker
            .check(vec!["a1"], filter_in(vec![&a1]))
            .check(vec!["a2"], filter_in(vec![&a2]))
            .check(vec!["a2b"], filter_in(vec![&a2b]))
            .check(vec!["a3"], filter_in(vec![&a3]))
            .check(vec!["a1", "a2"], filter_in(vec![&a1, &a2]))
            .check(vec!["a1", "a3"], filter_in(vec![&a1, &a3]));

        checker
            .check(vec!["a2", "a2b", "a3"], filter_not_in(vec![&a1]))
            .check(vec!["a1", "a2b", "a3"], filter_not_in(vec![&a2]))
            .check(vec!["a1", "a2", "a3"], filter_not_in(vec![&a2b]))
            .check(vec!["a1", "a2", "a2b"], filter_not_in(vec![&a3]))
            .check(vec!["a2b", "a3"], filter_not_in(vec![&a1, &a2]))
            .check(vec!["a2", "a2b"], filter_not_in(vec![&a1, &a3]));

        update_entity_at(
            conn,
            layout,
            &*FERRET_TYPE,
            vec![entity! { layout.input_schema =>
              id: "a1",
              name: "Test"
            }],
            1,
        );

        checker
            .check(vec!["a1", "a2", "a2b", "a3"], filter_block_gte(0))
            .check(vec!["a1"], filter_block_gte(1))
            .check(vec![], filter_block_gte(BLOCK_NUMBER_MAX));
    });
}
