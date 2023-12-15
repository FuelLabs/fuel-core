use std::{collections::BTreeSet, sync::Arc};

use diesel::{debug_query, pg::Pg};
use graph::{
    prelude::{r, serde_json as json, DeploymentHash, EntityFilter},
    schema::InputSchema,
};

use crate::{
    layout_for_tests::{make_dummy_site, Namespace},
    relational::{Catalog, ColumnType, Layout},
    relational_queries::FromColumnValue,
};

use crate::relational_queries::QueryFilter;

#[test]
fn gql_value_from_bytes() {
    const EXP: &str = "0xdeadbeef";

    let exp = r::Value::String(EXP.to_string());
    for s in ["deadbeef", "\\xdeadbeef", "0xdeadbeef"] {
        let act =
            r::Value::from_column_value(&ColumnType::Bytes, json::Value::String(s.to_string()))
                .unwrap();

        assert_eq!(exp, act);
    }
}

fn test_layout(gql: &str) -> Layout {
    let subgraph = DeploymentHash::new("subgraph").unwrap();
    let schema = InputSchema::parse(gql, subgraph.clone()).expect("Test schema invalid");
    let namespace = Namespace::new("sgd0815".to_owned()).unwrap();
    let site = Arc::new(make_dummy_site(subgraph, namespace, "anet".to_string()));
    let catalog =
        Catalog::for_tests(site.clone(), BTreeSet::new()).expect("Can not create catalog");
    Layout::new(site, &schema, catalog).expect("Failed to construct Layout")
}

#[track_caller]
fn filter_contains(filter: EntityFilter, sql: &str) {
    const SCHEMA: &str = "
    type Thing @entity {
        id: Bytes!,
        address: Bytes!,
        name: String
    }";
    let layout = test_layout(SCHEMA);
    let table = layout
        .table_for_entity(&layout.input_schema.entity_type("Thing").unwrap())
        .unwrap();
    let filter = QueryFilter::new(&filter, table.as_ref(), &layout, Default::default()).unwrap();
    let query = debug_query::<Pg, _>(&filter);
    assert!(
        query.to_string().contains(sql),
        "Expected query /{}/ to contain /{}/",
        query,
        sql
    );
}

#[test]
fn prefix() {
    // String prefixes
    let filter = EntityFilter::Equal("name".to_string(), "Bibi".into());
    filter_contains(
        filter,
        r#"left("name", 256) = left($1, 256) -- binds: ["Bibi"]"#,
    );

    let filter = EntityFilter::In("name".to_string(), vec!["Bibi".into(), "Julian".into()]);
    filter_contains(
        filter,
        r#"left("name", 256) in ($1, $2) -- binds: ["Bibi", "Julian"]"#,
    );

    // Bytes prefixes
    let filter = EntityFilter::Equal("address".to_string(), "0xbeef".into());
    filter_contains(
        filter,
        r#"substring("address", 1, 64) = substring($1, 1, 64)"#,
    );

    let filter = EntityFilter::In("address".to_string(), vec!["0xbeef".into()]);
    filter_contains(filter, r#"substring("address", 1, 64) in ($1)"#);
}
