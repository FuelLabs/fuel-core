//! Test relational schemas that use `Bytes` to store ids
use diesel::connection::SimpleConnection as _;
use diesel::pg::PgConnection;
use graph::components::store::write::RowGroup;
use graph::data::store::scalar;
use graph::data_source::CausalityRegion;
use graph::entity;
use graph::prelude::{BlockNumber, EntityModification, EntityQuery, MetricsRegistry, StoreError};
use graph::schema::{EntityKey, EntityType, InputSchema};
use hex_literal::hex;
use lazy_static::lazy_static;
use std::collections::BTreeSet;
use std::str::FromStr;
use std::{collections::BTreeMap, sync::Arc};

use graph::data::store::scalar::{BigDecimal, BigInt};
use graph::data::store::IdList;
use graph::prelude::{
    o, slog, web3::types::H256, AttributeNames, ChildMultiplicity, DeploymentHash, Entity,
    EntityCollection, EntityLink, EntityWindow, Logger, ParentLink, StopwatchMetrics,
    WindowAttribute, BLOCK_NUMBER_MAX,
};
use graph_store_postgres::{
    layout_for_tests::make_dummy_site,
    layout_for_tests::{Layout, Namespace},
};

use test_store::*;

const THINGS_GQL: &str = "
    type Thing @entity {
        id: Bytes!
        name: String!
        # We use these only in the EntityQuery tests
        parent: Thing
        children: [Thing!]
    }
";

lazy_static! {
    static ref THINGS_SUBGRAPH_ID: DeploymentHash = DeploymentHash::new("things").unwrap();
    static ref THINGS_SCHEMA: InputSchema =
        InputSchema::parse(THINGS_GQL, THINGS_SUBGRAPH_ID.clone())
            .expect("Failed to parse THINGS_GQL");
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
    static ref BEEF_ENTITY: Entity = entity! { THINGS_SCHEMA =>
        id: scalar::Bytes::from_str("deadbeef").unwrap(),
        name: "Beef",
    };
    static ref NAMESPACE: Namespace = Namespace::new("sgd0815".to_string()).unwrap();
    static ref THING_TYPE: EntityType = THINGS_SCHEMA.entity_type("Thing").unwrap();
    static ref MOCK_STOPWATCH: StopwatchMetrics = StopwatchMetrics::new(
        Logger::root(slog::Discard, o!()),
        THINGS_SUBGRAPH_ID.clone(),
        "test",
        Arc::new(MetricsRegistry::mock()),
        "test_shard".to_string()
    );
}

/// Removes test data from the database behind the store.
fn remove_test_data(conn: &PgConnection) {
    let query = format!("drop schema if exists {} cascade", NAMESPACE.as_str());
    conn.batch_execute(&query)
        .expect("Failed to drop test schema");
}

pub fn row_group_update(
    entity_type: &EntityType,
    block: BlockNumber,
    data: impl IntoIterator<Item = (EntityKey, Entity)>,
) -> RowGroup {
    let mut group = RowGroup::new(entity_type.clone(), false);
    for (key, data) in data {
        group
            .push(EntityModification::overwrite(key, data, block), block)
            .unwrap();
    }
    group
}

pub fn row_group_insert(
    entity_type: &EntityType,
    block: BlockNumber,
    data: impl IntoIterator<Item = (EntityKey, Entity)>,
) -> RowGroup {
    let mut group = RowGroup::new(entity_type.clone(), false);
    for (key, data) in data {
        group
            .push(EntityModification::insert(key, data, block), block)
            .unwrap();
    }
    group
}

pub fn row_group_delete(
    entity_type: &EntityType,
    block: BlockNumber,
    data: impl IntoIterator<Item = EntityKey>,
) -> RowGroup {
    let mut group = RowGroup::new(entity_type.clone(), false);
    for key in data {
        group
            .push(EntityModification::remove(key, block), block)
            .unwrap();
    }
    group
}

fn insert_entity(conn: &PgConnection, layout: &Layout, entity_type: &str, entity: Entity) {
    let entity_type = layout.input_schema.entity_type(entity_type).unwrap();
    let key = entity_type.key(entity.id());

    let entities = vec![(key.clone(), entity)];
    let group = row_group_insert(&entity_type, 0, entities);
    let errmsg = format!("Failed to insert entity {}[{}]", entity_type, key.entity_id);
    layout.insert(conn, &group, &MOCK_STOPWATCH).expect(&errmsg);
}

fn insert_thing(conn: &PgConnection, layout: &Layout, id: &str, name: &str) {
    insert_entity(
        conn,
        layout,
        "Thing",
        entity! { layout.input_schema =>
            id: id,
            name: name
        },
    );
}

fn create_schema(conn: &PgConnection) -> Layout {
    let schema = InputSchema::parse(THINGS_GQL, THINGS_SUBGRAPH_ID.clone()).unwrap();

    let query = format!("create schema {}", NAMESPACE.as_str());
    conn.batch_execute(&query).unwrap();

    let site = make_dummy_site(
        THINGS_SUBGRAPH_ID.clone(),
        NAMESPACE.clone(),
        NETWORK_NAME.to_string(),
    );
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

fn run_test<F>(test: F)
where
    F: FnOnce(&PgConnection, &Layout),
{
    run_test_with_conn(|conn| {
        // Reset state before starting
        remove_test_data(conn);

        // Seed database with test data
        let layout = create_schema(conn);

        // Run test
        test(conn, &layout);
    });
}

#[test]
fn bad_id() {
    run_test(|conn, layout| {
        fn find(
            conn: &PgConnection,
            layout: &Layout,
            id: &str,
        ) -> Result<Option<Entity>, StoreError> {
            let key = THING_TYPE.parse_key(id)?;
            layout.find(conn, &key, BLOCK_NUMBER_MAX)
        }

        // We test that we get errors for various strings that are not
        // valid 'Bytes' strings; we use `find` to force the conversion
        // from String -> Bytes internally
        let res = find(conn, layout, "bad");
        assert!(res.is_err());
        assert_eq!(
            "store error: Odd number of digits",
            res.err().unwrap().to_string()
        );

        // We do not allow the `\x` prefix that Postgres uses
        let res = find(conn, layout, "\\xbadd");
        assert!(res.is_err());
        assert_eq!(
            "store error: Invalid character \'\\\\\' at position 0",
            res.err().unwrap().to_string()
        );

        // Having the '0x' prefix is ok
        let res = find(conn, layout, "0xbadd");
        assert!(res.is_ok());

        // Using non-hex characters is also bad
        let res = find(conn, layout, "nope");
        assert!(res.is_err());
        assert_eq!(
            "store error: Invalid character \'n\' at position 0",
            res.err().unwrap().to_string()
        );
    });
}

#[test]
fn find() {
    run_test(|conn, layout| {
        fn find_entity(conn: &PgConnection, layout: &Layout, id: &str) -> Option<Entity> {
            let key = THING_TYPE.parse_key(id).unwrap();
            layout
                .find(conn, &key, BLOCK_NUMBER_MAX)
                .expect(&format!("Failed to read Thing[{}]", id))
        }

        const ID: &str = "deadbeef";
        const NAME: &str = "Beef";
        insert_thing(conn, layout, ID, NAME);

        // Happy path: find existing entity
        let entity = find_entity(conn, layout, ID).unwrap();
        assert_entity_eq!(scrub(&BEEF_ENTITY), entity);
        assert!(CausalityRegion::from_entity(&entity) == CausalityRegion::ONCHAIN);

        // Find non-existing entity
        let entity = find_entity(conn, layout, "badd");
        assert!(entity.is_none());
    });
}

#[test]
fn find_many() {
    run_test(|conn, layout| {
        const ID: &str = "0xdeadbeef";
        const NAME: &str = "Beef";
        const ID2: &str = "0xdeadbeef02";
        const NAME2: &str = "Moo";
        insert_thing(conn, layout, ID, NAME);
        insert_thing(conn, layout, ID2, NAME2);

        let mut id_map = BTreeMap::default();
        let ids = IdList::try_from_iter(
            &*THING_TYPE,
            vec![ID, ID2, "badd"]
                .into_iter()
                .map(|id| THING_TYPE.parse_id(id).unwrap()),
        )
        .unwrap();
        id_map.insert((THING_TYPE.clone(), CausalityRegion::ONCHAIN), ids);

        let entities = layout
            .find_many(conn, &id_map, BLOCK_NUMBER_MAX)
            .expect("Failed to read many things");
        assert_eq!(2, entities.len());

        let id_key = THING_TYPE.parse_key(ID).unwrap();
        let id2_key = THING_TYPE.parse_key(ID2).unwrap();
        assert!(entities.contains_key(&id_key), "Missing ID");
        assert!(entities.contains_key(&id2_key), "Missing ID2");
    });
}

#[test]
fn update() {
    run_test(|conn, layout| {
        insert_entity(conn, layout, "Thing", BEEF_ENTITY.clone());

        // Update the entity
        let mut entity = BEEF_ENTITY.clone();
        entity.set("name", "Moo").unwrap();
        let key = THING_TYPE.key(entity.id());

        let entity_id = entity.id();
        let entity_type = key.entity_type.clone();
        let entities = vec![(key, entity.clone())];
        let group = row_group_update(&entity_type, 1, entities);
        layout
            .update(conn, &group, &MOCK_STOPWATCH)
            .expect("Failed to update");

        let actual = layout
            .find(conn, &THING_TYPE.key(entity_id), BLOCK_NUMBER_MAX)
            .expect("Failed to read Thing[deadbeef]")
            .unwrap();

        assert_entity_eq!(entity, actual);
    });
}

#[test]
fn delete() {
    run_test(|conn, layout| {
        const TWO_ID: &str = "deadbeef02";

        insert_entity(conn, layout, "Thing", BEEF_ENTITY.clone());
        let mut two = BEEF_ENTITY.clone();
        two.set("id", TWO_ID).unwrap();
        insert_entity(conn, layout, "Thing", two);

        // Delete where nothing is getting deleted
        let key = THING_TYPE.parse_key("ffff").unwrap();
        let entity_type = key.entity_type.clone();
        let mut entity_keys = vec![key.clone()];
        let group = row_group_delete(&entity_type, 1, entity_keys.clone());
        let count = layout
            .delete(conn, &group, &MOCK_STOPWATCH)
            .expect("Failed to delete");
        assert_eq!(0, count);

        // Delete entity two
        entity_keys
            .get_mut(0)
            .map(|key| key.entity_id = entity_type.parse_id(TWO_ID).unwrap())
            .expect("Failed to update entity types");
        let group = row_group_delete(&entity_type, 1, entity_keys);
        let count = layout
            .delete(conn, &group, &MOCK_STOPWATCH)
            .expect("Failed to delete");
        assert_eq!(1, count);
    });
}

//
// Test Layout::query to check that query generation is syntactically sound
//
const ROOT: &str = "0xdead00";
const CHILD1: &str = "0xbabe01";
const CHILD2: &str = "0xbabe02";
const GRANDCHILD1: &str = "0xfafa01";
const GRANDCHILD2: &str = "0xfafa02";

/// Create a set of test data that forms a tree through the `parent` and `children` attributes.
/// The tree has this form:
///
///   root
///     +- child1
///          +- grandchild1
///     +- child2
///          +- grandchild2
///
fn make_thing_tree(conn: &PgConnection, layout: &Layout) -> (Entity, Entity, Entity) {
    let root = entity! { layout.input_schema =>
        id: ROOT,
        name: "root",
        children: vec!["babe01", "babe02"]
    };
    let child1 = entity! { layout.input_schema =>
        id: CHILD1,
        name: "child1",
        parent: "dead00",
        children: vec![GRANDCHILD1]
    };
    let child2 = entity! { layout.input_schema =>
        id: CHILD2,
        name: "child2",
        parent: "dead00",
        children: vec![GRANDCHILD1]
    };
    let grand_child1 = entity! { layout.input_schema =>
        id: GRANDCHILD1,
        name: "grandchild1",
        parent: CHILD1
    };
    let grand_child2 = entity! { layout.input_schema =>
        id: GRANDCHILD2,
        name: "grandchild2",
        parent: CHILD2
    };

    insert_entity(conn, layout, "Thing", root.clone());
    insert_entity(conn, layout, "Thing", child1.clone());
    insert_entity(conn, layout, "Thing", child2.clone());
    insert_entity(conn, layout, "Thing", grand_child1);
    insert_entity(conn, layout, "Thing", grand_child2);
    (root, child1, child2)
}

#[test]
fn query() {
    fn fetch(conn: &PgConnection, layout: &Layout, coll: EntityCollection) -> Vec<String> {
        let id = DeploymentHash::new("QmXW3qvxV7zXnwRntpj7yoK8HZVtaraZ67uMqaLRvXdxha").unwrap();
        let query = EntityQuery::new(id, BLOCK_NUMBER_MAX, coll).first(10);
        layout
            .query::<Entity>(&LOGGER, conn, query)
            .map(|(entities, _)| entities)
            .expect("the query succeeds")
            .into_iter()
            .map(|e| e.id().to_string())
            .collect::<Vec<_>>()
    }

    run_test(|conn, layout| {
        // This test exercises the different types of queries we generate;
        // the type of query is based on knowledge of what the test data
        // looks like, not on just an inference from the GraphQL model.
        // Especially the multiplicity for type A and B queries is determined
        // by knowing whether there are one or many entities per parent
        // in the test data
        make_thing_tree(conn, layout);

        // See https://graphprotocol.github.io/rfcs/engineering-plans/0001-graphql-query-prefetching.html#handling-parentchild-relationships
        // for a discussion of the various types of relationships and queries

        // EntityCollection::All
        let coll = EntityCollection::All(vec![(THING_TYPE.clone(), AttributeNames::All)]);
        let things = fetch(conn, layout, coll);
        assert_eq!(vec![CHILD1, CHILD2, ROOT, GRANDCHILD1, GRANDCHILD2], things);

        // EntityCollection::Window, type A, many
        //   things(where: { children_contains: [CHILD1] }) { id }
        let coll = EntityCollection::Window(vec![EntityWindow {
            child_type: THING_TYPE.clone(),
            ids: THING_TYPE.parse_ids(vec![CHILD1]).unwrap(),
            link: EntityLink::Direct(
                WindowAttribute::List("children".to_string()),
                ChildMultiplicity::Many,
            ),
            column_names: AttributeNames::All,
        }]);
        let things = fetch(conn, layout, coll);
        assert_eq!(vec![ROOT], things);

        // EntityCollection::Window, type A, single
        //   things(where: { children_contains: [GRANDCHILD1, GRANDCHILD2] }) { id }
        let coll = EntityCollection::Window(vec![EntityWindow {
            child_type: THING_TYPE.clone(),
            ids: THING_TYPE
                .parse_ids(vec![GRANDCHILD1, GRANDCHILD2])
                .unwrap(),
            link: EntityLink::Direct(
                WindowAttribute::List("children".to_string()),
                ChildMultiplicity::Single,
            ),
            column_names: AttributeNames::All,
        }]);
        let things = fetch(conn, layout, coll);
        assert_eq!(vec![CHILD1, CHILD2], things);

        // EntityCollection::Window, type B, many
        //   things(where: { parent: [ROOT] }) { id }
        let coll = EntityCollection::Window(vec![EntityWindow {
            child_type: THING_TYPE.clone(),
            ids: THING_TYPE.parse_ids(vec![ROOT]).unwrap(),
            link: EntityLink::Direct(
                WindowAttribute::Scalar("parent".to_string()),
                ChildMultiplicity::Many,
            ),
            column_names: AttributeNames::All,
        }]);
        let things = fetch(conn, layout, coll);
        assert_eq!(vec![CHILD1, CHILD2], things);

        // EntityCollection::Window, type B, single
        //   things(where: { parent: [CHILD1, CHILD2] }) { id }
        let coll = EntityCollection::Window(vec![EntityWindow {
            child_type: THING_TYPE.clone(),
            ids: THING_TYPE.parse_ids(vec![CHILD1, CHILD2]).unwrap(),
            link: EntityLink::Direct(
                WindowAttribute::Scalar("parent".to_string()),
                ChildMultiplicity::Single,
            ),
            column_names: AttributeNames::All,
        }]);
        let things = fetch(conn, layout, coll);
        assert_eq!(vec![GRANDCHILD1, GRANDCHILD2], things);

        // EntityCollection::Window, type C
        //   things { children { id } }
        // This is the inner 'children' query
        let coll = EntityCollection::Window(vec![EntityWindow {
            child_type: THING_TYPE.clone(),
            ids: THING_TYPE.parse_ids(vec![ROOT]).unwrap(),
            link: EntityLink::Parent(
                THING_TYPE.clone(),
                ParentLink::List(vec![THING_TYPE.parse_ids(vec![CHILD1, CHILD2]).unwrap()]),
            ),
            column_names: AttributeNames::All,
        }]);
        let things = fetch(conn, layout, coll);
        assert_eq!(vec![CHILD1, CHILD2], things);

        // EntityCollection::Window, type D
        //   things { parent { id } }
        // This is the inner 'parent' query
        let coll = EntityCollection::Window(vec![EntityWindow {
            child_type: THING_TYPE.clone(),
            ids: THING_TYPE.parse_ids(vec![CHILD1, CHILD2]).unwrap(),
            link: EntityLink::Parent(
                THING_TYPE.clone(),
                ParentLink::Scalar(THING_TYPE.parse_ids(vec![ROOT, ROOT]).unwrap()),
            ),
            column_names: AttributeNames::All,
        }]);
        let things = fetch(conn, layout, coll);
        assert_eq!(vec![ROOT, ROOT], things);
    });
}
