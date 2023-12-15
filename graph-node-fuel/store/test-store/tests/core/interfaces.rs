// Tests for graphql interfaces.

use graph::entity;
use graph::schema::InputSchema;
use pretty_assertions::assert_eq;

use graph::data::graphql::object;
use graph::{data::query::QueryTarget, prelude::*};
use test_store::*;

// `entities` is `(entity, type)`.
async fn insert_and_query(
    subgraph_id: &str,
    schema: &str,
    entities: Vec<(&str, Entity)>,
    query: &str,
) -> Result<QueryResult, StoreError> {
    let subgraph_id = DeploymentHash::new(subgraph_id).unwrap();
    let deployment = create_test_subgraph(&subgraph_id, schema).await;
    let schema = InputSchema::parse(schema, subgraph_id.clone()).unwrap();
    let entities = entities
        .into_iter()
        .map(|(entity_type, data)| (schema.entity_type(entity_type).unwrap(), data))
        .collect();

    insert_entities(&deployment, entities).await?;

    let document = graphql_parser::parse_query(query).unwrap().into_static();
    let target = QueryTarget::Deployment(subgraph_id, Default::default());
    let query = Query::new(document, None, false);
    Ok(execute_subgraph_query(query, target)
        .await
        .first()
        .unwrap()
        .duplicate())
}

/// Extract the data from a `QueryResult`, and panic if it has errors
macro_rules! extract_data {
    ($result: expr) => {
        match $result.to_result() {
            Err(errors) => panic!("Unexpected errors return for query: {:#?}", errors),
            Ok(data) => data,
        }
    };
}

#[tokio::test]
async fn one_interface_zero_entities() {
    let subgraph_id = "oneInterfaceZeroEntities";
    let schema = "interface Legged { legs: Int }
                  type Animal implements Legged @entity { id: ID!, legs: Int }";

    let query = "query { leggeds(first: 100) { legs } }";

    let res = insert_and_query(subgraph_id, schema, vec![], query)
        .await
        .unwrap();

    let data = extract_data!(res).unwrap();
    let exp = object! { leggeds: Vec::<r::Value>::new() };
    assert_eq!(data, exp);
}

#[tokio::test]
async fn one_interface_one_entity() {
    let subgraph_id = "oneInterfaceOneEntity";
    let document = "interface Legged { legs: Int }
                  type Animal implements Legged @entity { id: ID!, legs: Int }";
    let schema = InputSchema::raw(document, subgraph_id);

    let entity = ("Animal", entity! { schema => id: "1", legs: 3 });

    // Collection query.
    let query = "query { leggeds(first: 100) { legs } }";
    let res = insert_and_query(subgraph_id, document, vec![entity], query)
        .await
        .unwrap();
    let data = extract_data!(res).unwrap();
    let exp = object! { leggeds: vec![ object!{ legs: 3 }]};
    assert_eq!(data, exp);

    // Query by ID.
    let query = "query { legged(id: \"1\") { legs } }";
    let res = insert_and_query(subgraph_id, document, vec![], query)
        .await
        .unwrap();
    let data = extract_data!(res).unwrap();
    let exp = object! { legged: object! { legs: 3 }};
    assert_eq!(data, exp);
}

#[tokio::test]
async fn one_interface_one_entity_typename() {
    let subgraph_id = "oneInterfaceOneEntityTypename";
    let document = "interface Legged { legs: Int }
                  type Animal implements Legged @entity { id: ID!, legs: Int }";
    let schema = InputSchema::raw(document, subgraph_id);

    let entity = ("Animal", entity! { schema => id: "1", legs: 3 });

    let query = "query { leggeds(first: 100) { __typename } }";

    let res = insert_and_query(subgraph_id, document, vec![entity], query)
        .await
        .unwrap();
    let data = extract_data!(res).unwrap();
    let exp = object! { leggeds: vec![ object!{ __typename: "Animal" } ]};
    assert_eq!(data, exp);
}

#[tokio::test]
async fn one_interface_multiple_entities() {
    let subgraph_id = "oneInterfaceMultipleEntities";
    let document = "interface Legged { legs: Int }
                  type Animal implements Legged @entity { id: ID!, legs: Int }
                  type Furniture implements Legged @entity { id: ID!, legs: Int }
                  ";
    let schema = InputSchema::raw(document, subgraph_id);

    let animal = ("Animal", entity! { schema => id: "1", legs: 3 });
    let furniture = ("Furniture", entity! { schema => id: "2", legs: 4 });

    let query = "query { leggeds(first: 100, orderBy: legs) { legs } }";

    let res = insert_and_query(subgraph_id, document, vec![animal, furniture], query)
        .await
        .unwrap();
    let data = extract_data!(res).unwrap();
    let exp = object! { leggeds: vec![ object! { legs: 3 }, object! { legs: 4 }]};
    assert_eq!(data, exp);

    // Test for support issue #32.
    let query = "query { legged(id: \"2\") { legs } }";
    let res = insert_and_query(subgraph_id, document, vec![], query)
        .await
        .unwrap();
    let data = extract_data!(res).unwrap();
    let exp = object! { legged: object! { legs: 4 }};
    assert_eq!(data, exp);
}

#[tokio::test]
async fn reference_interface() {
    let subgraph_id = "ReferenceInterface";
    let document = "type Leg @entity { id: ID! }
                  interface Legged { leg: Leg }
                  type Animal implements Legged @entity { id: ID!, leg: Leg }";
    let schema = InputSchema::raw(document, subgraph_id);

    let query = "query { leggeds(first: 100) { leg { id } } }";

    let leg = ("Leg", entity! { schema => id: "1" });
    let animal = ("Animal", entity! { schema => id: "1", leg: 1 });

    let res = insert_and_query(subgraph_id, document, vec![leg, animal], query)
        .await
        .unwrap();

    let data = extract_data!(res).unwrap();
    let exp = object! { leggeds: vec![ object!{ leg: object! { id: "1" } }] };
    assert_eq!(data, exp);
}

#[tokio::test]
async fn reference_interface_derived() {
    // Test the different ways in which interface implementations
    // can reference another entity
    let subgraph_id = "ReferenceInterfaceDerived";
    let document = "
    type Transaction @entity {
        id: ID!,
        buyEvent: BuyEvent!,
        sellEvents: [SellEvent!]!,
        giftEvent: [GiftEvent!]! @derivedFrom(field: \"transaction\"),
    }

    interface Event {
        id: ID!,
        transaction: Transaction!
    }

    type BuyEvent implements Event @entity {
        id: ID!,
        # Derived, but only one buyEvent per Transaction
        transaction: Transaction! @derivedFrom(field: \"buyEvent\")
    }

    type SellEvent implements Event @entity {
        id: ID!
        # Derived, many sellEvents per Transaction
        transaction: Transaction! @derivedFrom(field: \"sellEvents\")
    }

    type GiftEvent implements Event @entity {
        id: ID!,
        # Store the transaction directly
        transaction: Transaction!
    }";
    let schema = InputSchema::raw(document, subgraph_id);

    let query = "query { events { id transaction { id } } }";

    let buy = ("BuyEvent", entity! { schema => id: "buy" });
    let sell1 = ("SellEvent", entity! { schema => id: "sell1" });
    let sell2 = ("SellEvent", entity! { schema => id: "sell2" });
    let gift = (
        "GiftEvent",
        entity! { schema => id: "gift", transaction: "txn" },
    );
    let txn = (
        "Transaction",
        entity! { schema => id: "txn", buyEvent: "buy", sellEvents: vec!["sell1", "sell2"] },
    );

    let entities = vec![buy, sell1, sell2, gift, txn];
    let res = insert_and_query(subgraph_id, document, entities.clone(), query)
        .await
        .unwrap();

    let data = extract_data!(res).unwrap();
    let exp = object! {
        events: vec![
            object! { id: "buy", transaction: object! { id: "txn" } },
            object! { id: "gift", transaction: object! { id: "txn" } },
            object! { id: "sell1", transaction: object! { id: "txn" } },
            object! { id: "sell2", transaction: object! { id: "txn" } }
        ]
    };
    assert_eq!(data, exp);
}

#[tokio::test]
async fn follow_interface_reference_invalid() {
    let subgraph_id = "FollowInterfaceReferenceInvalid";
    let schema = "interface Legged { legs: Int! }
                  type Animal implements Legged @entity {
                    id: ID!
                    legs: Int!
                    parent: Legged
                  }";

    let query = "query { legged(id: \"child\") { parent { id } } }";

    let res = insert_and_query(subgraph_id, schema, vec![], query)
        .await
        .unwrap();

    // Depending on whether `ENABLE_GRAPHQL_VALIDATIONS` is set or not, we
    // get different errors
    match &res.to_result().unwrap_err()[0] {
        QueryError::ExecutionError(QueryExecutionError::ValidationError(_, error_message)) => {
            assert_eq!(
                error_message,
                "Cannot query field \"parent\" on type \"Legged\"."
            );
        }
        QueryError::ExecutionError(QueryExecutionError::UnknownField(_, type_name, field_name)) => {
            assert_eq!(type_name, "Legged");
            assert_eq!(field_name, "parent");
        }
        e => panic!("error `{}` is not the expected one", e),
    }
}

#[tokio::test]
async fn follow_interface_reference() {
    let subgraph_id = "FollowInterfaceReference";
    let document = "interface Legged { id: ID!, legs: Int! }
                  type Animal implements Legged @entity {
                    id: ID!
                    legs: Int!
                    parent: Legged
                  }";
    let schema = InputSchema::raw(document, subgraph_id);

    let query = "query { legged(id: \"child\") { ... on Animal { parent { id } } } }";

    let parent = (
        "Animal",
        entity! { schema => id: "parent", legs: 4, parent: Value::Null },
    );
    let child = (
        "Animal",
        entity! { schema => id: "child", legs: 3, parent: "parent" },
    );

    let res = insert_and_query(subgraph_id, document, vec![parent, child], query)
        .await
        .unwrap();

    let data = extract_data!(res).unwrap();
    let exp = object! {
        legged: object! { parent: object! { id: "parent" } }
    };
    assert_eq!(data, exp)
}

#[tokio::test]
async fn conflicting_implementors_id() {
    let subgraph_id = "ConflictingImplementorsId";
    let document = "interface Legged { legs: Int }
                  type Animal implements Legged @entity { id: ID!, legs: Int }
                  type Furniture implements Legged @entity { id: ID!, legs: Int }
                  ";
    let schema = InputSchema::raw(document, subgraph_id);

    let animal = ("Animal", entity! { schema => id: "1", legs: 3 });
    let furniture = ("Furniture", entity! { schema => id: "1", legs: 3 });

    let query = "query { leggeds(first: 100) { legs } }";

    let res = insert_and_query(subgraph_id, document, vec![animal, furniture], query).await;

    let msg = res.unwrap_err().to_string();
    // We don't know in which order the two entities get inserted; the two
    // error messages only differ in who gets inserted first
    const EXPECTED1: &str =
        "tried to set entity of type `Furniture` with ID \"1\" but an entity of type `Animal`, \
         which has an interface in common with `Furniture`, exists with the same ID";
    const EXPECTED2: &str =
        "tried to set entity of type `Animal` with ID \"1\" but an entity of type `Furniture`, \
         which has an interface in common with `Animal`, exists with the same ID";

    assert!(msg == EXPECTED1 || msg == EXPECTED2);
}

#[tokio::test]
async fn derived_interface_relationship() {
    let subgraph_id = "DerivedInterfaceRelationship";
    let document = "interface ForestDweller { id: ID!, forest: Forest }
                  type Animal implements ForestDweller @entity { id: ID!, forest: Forest }
                  type Forest @entity { id: ID!, dwellers: [ForestDweller]! @derivedFrom(field: \"forest\") }
                  ";
    let schema = InputSchema::raw(document, subgraph_id);

    let forest = ("Forest", entity! { schema => id: "1" });
    let animal = ("Animal", entity! { schema => id: "1", forest: "1" });

    let query = "query { forests(first: 100) { dwellers(first: 100) { id } } }";

    let res = insert_and_query(subgraph_id, document, vec![forest, animal], query)
        .await
        .unwrap();
    let data = extract_data!(res);
    assert_eq!(
        data.unwrap().to_string(),
        "{forests: [{dwellers: [{id: \"1\"}]}]}"
    );
}

#[tokio::test]
async fn two_interfaces() {
    let subgraph_id = "TwoInterfaces";
    let document = "interface IFoo { foo: String! }
                  interface IBar { bar: Int! }

                  type A implements IFoo @entity { id: ID!, foo: String! }
                  type B implements IBar @entity { id: ID!, bar: Int! }

                  type AB implements IFoo & IBar @entity { id: ID!, foo: String!, bar: Int! }
                  ";
    let schema = InputSchema::raw(document, subgraph_id);

    let a = ("A", entity! { schema => id: "1", foo: "bla" });
    let b = ("B", entity! { schema => id: "1", bar: 100 });
    let ab = ("AB", entity! { schema => id: "2", foo: "ble", bar: 200 });

    let query = "query {
                    ibars(first: 100, orderBy: bar) { bar }
                    ifoos(first: 100, orderBy: foo) { foo }
                }";
    let res = insert_and_query(subgraph_id, document, vec![a, b, ab], query)
        .await
        .unwrap();
    let data = extract_data!(res).unwrap();
    let exp = object! {
        ibars: vec![ object! { bar: 100 }, object! { bar: 200 }],
        ifoos: vec![ object! { foo: "bla" }, object! { foo: "ble" } ]
    };
    assert_eq!(data, exp);
}

#[tokio::test]
async fn interface_non_inline_fragment() {
    let subgraph_id = "interfaceNonInlineFragment";
    let document = "interface Legged { legs: Int }
                  type Animal implements Legged @entity { id: ID!, name: String, legs: Int }";
    let schema = InputSchema::raw(document, subgraph_id);

    let entity = (
        "Animal",
        entity! { schema => id: "1", name: "cow", legs: 3 },
    );

    // Query only the fragment.
    let query = "query { leggeds { ...frag } } fragment frag on Animal { name }";
    let res = insert_and_query(subgraph_id, document, vec![entity], query)
        .await
        .unwrap();
    let data = extract_data!(res).unwrap();
    let exp = object! { leggeds: vec![ object! { name: "cow" } ]};
    assert_eq!(data, exp);

    // Query the fragment and something else.
    let query = "query { leggeds { legs, ...frag } } fragment frag on Animal { name }";
    let res = insert_and_query(subgraph_id, document, vec![], query)
        .await
        .unwrap();
    let data = extract_data!(res).unwrap();
    let exp = object! { leggeds: vec![ object!{ legs: 3, name: "cow" } ]};
    assert_eq!(data, exp);
}

#[tokio::test]
async fn interface_inline_fragment() {
    let subgraph_id = "interfaceInlineFragment";
    let document = "interface Legged { legs: Int }
                  type Animal implements Legged @entity { id: ID!, name: String, legs: Int }
                  type Bird implements Legged @entity { id: ID!, airspeed: Int, legs: Int }";
    let schema = InputSchema::raw(document, subgraph_id);

    let animal = (
        "Animal",
        entity! { schema => id: "1", name: "cow", legs: 4 },
    );
    let bird = ("Bird", entity! { schema => id: "2", airspeed: 24, legs: 2 });

    let query =
        "query { leggeds(orderBy: legs) { ... on Animal { name } ...on Bird { airspeed } } }";
    let res = insert_and_query(subgraph_id, document, vec![animal, bird], query)
        .await
        .unwrap();
    let data = extract_data!(res).unwrap();
    let exp = object! { leggeds: vec![ object!{ airspeed: 24 }, object! { name: "cow" }]};
    assert_eq!(data, exp);
}

#[tokio::test]
async fn interface_inline_fragment_with_subquery() {
    let subgraph_id = "InterfaceInlineFragmentWithSubquery";
    let document = "
        interface Legged { legs: Int }
        type Parent @entity {
          id: ID!
        }
        type Animal implements Legged @entity {
          id: ID!
          name: String
          legs: Int
          parent: Parent
        }
        type Bird implements Legged @entity {
          id: ID!
          airspeed: Int
          legs: Int
          parent: Parent
        }
    ";
    let schema = InputSchema::raw(document, subgraph_id);

    let mama_cow = ("Parent", entity! { schema =>  id: "mama_cow" });
    let cow = (
        "Animal",
        entity! { schema =>  id: "1", name: "cow", legs: 4, parent: "mama_cow" },
    );

    let mama_bird = ("Parent", entity! { schema =>  id: "mama_bird" });
    let bird = (
        "Bird",
        entity! { schema =>  id: "2", airspeed: 5, legs: 2, parent: "mama_bird" },
    );

    let query = "query { leggeds(orderBy: legs) { legs ... on Bird { airspeed parent { id } } } }";
    let res = insert_and_query(
        subgraph_id,
        document,
        vec![cow, mama_cow, bird, mama_bird],
        query,
    )
    .await
    .unwrap();
    let data = extract_data!(res).unwrap();
    let exp = object! {
        leggeds: vec![ object!{ legs: 2, airspeed: 5, parent: object! { id: "mama_bird" } },
                       object!{ legs: 4 }]
    };
    assert_eq!(data, exp);
}

#[tokio::test]
async fn invalid_fragment() {
    let subgraph_id = "InvalidFragment";
    let schema = "interface Legged { legs: Int! }
                  type Animal implements Legged @entity {
                    id: ID!
                    name: String!
                    legs: Int!
                    parent: Legged
                  }";

    let query = "query { legged(id: \"child\") { ...{ name } } }";

    let res = insert_and_query(subgraph_id, schema, vec![], query)
        .await
        .unwrap();

    match &res.to_result().unwrap_err()[0] {
        QueryError::ExecutionError(QueryExecutionError::ValidationError(_, error_message)) => {
            assert_eq!(
                error_message,
                "Cannot query field \"name\" on type \"Legged\"."
            );
        }
        QueryError::ExecutionError(QueryExecutionError::UnknownField(_, type_name, field_name)) => {
            assert_eq!(type_name, "Legged");
            assert_eq!(field_name, "name");
        }
        e => panic!("error {} is not the expected one", e),
    }
}

#[tokio::test]
async fn alias() {
    let subgraph_id = "Alias";
    let document = "interface Legged { id: ID!, legs: Int! }
                  type Animal implements Legged @entity {
                    id: ID!
                    legs: Int!
                    parent: Legged
                  }";
    let schema = InputSchema::raw(document, subgraph_id);

    let query = "query {
                    l: legged(id: \"child\") {
                        ... on Animal {
                            p: parent {
                                i: id,
                                t: __typename,
                                __typename
                            }
                        }
                    }
            }";

    let parent = (
        "Animal",
        entity! { schema =>  id: "parent", legs: 4, parent: Value::Null },
    );
    let child = (
        "Animal",
        entity! { schema =>  id: "child", legs: 3, parent: "parent" },
    );

    let res = insert_and_query(subgraph_id, document, vec![parent, child], query)
        .await
        .unwrap();
    let data = extract_data!(res).unwrap();
    assert_eq!(
        data,
        object! {
            l: object! {
                p: object! {
                    i: "parent",
                    t: "Animal",
                    __typename: "Animal"
                }
            }
        }
    )
}

#[tokio::test]
async fn fragments_dont_panic() {
    let subgraph_id = "FragmentsDontPanic";
    let document = "
      type Parent @entity {
        id: ID!
        child: Child
      }

      type Child @entity {
        id: ID!
      }
    ";
    let schema = InputSchema::raw(document, subgraph_id);

    let query = "
        query {
            parents {
                ...on Parent {
                    child {
                        id
                    }
                }
                ...Frag
                child {
                    id
                }
            }
        }

        fragment Frag on Parent {
            child {
                id
            }
        }
    ";

    // The panic manifests if two parents exist.
    let parent = ("Parent", entity! { schema => id: "p", child: "c" });
    let parent2 = ("Parent", entity! { schema => id: "p2", child: Value::Null });
    let child = ("Child", entity! { schema => id:"c" });

    let res = insert_and_query(subgraph_id, document, vec![parent, parent2, child], query)
        .await
        .unwrap();

    let data = extract_data!(res).unwrap();
    assert_eq!(
        data,
        object! {
            parents: vec![
                object! {
                    child: object! {
                        id: "c",
                    }
                },
                object! {
                    child: r::Value::Null
                }
            ]
        }
    )
}

// See issue #1816
#[tokio::test]
async fn fragments_dont_duplicate_data() {
    let subgraph_id = "FragmentsDupe";
    let document = "
      type Parent @entity {
        id: ID!
        children: [Child!]!
      }

      type Child @entity {
        id: ID!
      }
    ";
    let schema = InputSchema::raw(document, subgraph_id);

    let query = "
        query {
            parents {
                ...Frag
                children {
                    id
                }
            }
        }

        fragment Frag on Parent {
            children {
                id
            }
        }
    ";

    // This bug manifests if two parents exist.
    let parent = ("Parent", entity! { schema => id: "p", children: vec!["c"] });
    let parent2 = (
        "Parent",
        entity! { schema => id: "b", children: Vec::<String>::new() },
    );
    let child = ("Child", entity! { schema => id:"c" });

    let res = insert_and_query(subgraph_id, document, vec![parent, parent2, child], query)
        .await
        .unwrap();

    let data = extract_data!(res).unwrap();
    assert_eq!(
        data,
        object! {
            parents: vec![
                object! {
                    children: Vec::<r::Value>::new()
                },
                object! {
                    children: vec![
                        object! {
                            id: "c",
                        }
                    ]
                }
            ]
        }
    )
}

// See also: e0d6da3e-60cf-41a5-b83c-b60a7a766d4a
#[tokio::test]
async fn redundant_fields() {
    let subgraph_id = "RedundantFields";
    let document = "interface Legged { id: ID!, parent: Legged }
                  type Animal implements Legged @entity {
                    id: ID!
                    parent: Legged
                  }";
    let schema = InputSchema::raw(document, subgraph_id);

    let query = "query {
                    leggeds {
                        parent { id }
                        ...on Animal {
                            parent { id }
                        }
                    }
            }";

    let parent = (
        "Animal",
        entity! { schema => id: "parent", parent: Value::Null },
    );
    let child = (
        "Animal",
        entity! { schema => id: "child", parent: "parent" },
    );

    let res = insert_and_query(subgraph_id, document, vec![parent, child], query)
        .await
        .unwrap();

    let data = extract_data!(res).unwrap();
    assert_eq!(
        data,
        object! {
            leggeds: vec![
                object! {
                    parent: object! {
                        id: "parent",
                    },
                },
                object! {
                    parent: r::Value::Null
                }
            ]
        }
    )
}

#[tokio::test]
async fn fragments_merge_selections() {
    let subgraph_id = "FragmentsMergeSelections";
    let document = "
      type Parent @entity {
        id: ID!
        children: [Child!]!
      }

      type Child @entity {
        id: ID!
        foo: Int!
      }
    ";
    let schema = InputSchema::raw(document, subgraph_id);

    let query = "
        query {
            parents {
                ...Frag
                children {
                    id
                }
            }
        }

        fragment Frag on Parent {
            children {
                foo
            }
        }
    ";

    let parent = ("Parent", entity! { schema => id: "p", children: vec!["c"] });
    let child = ("Child", entity! { schema => id: "c", foo: 1 });

    let res = insert_and_query(subgraph_id, document, vec![parent, child], query)
        .await
        .unwrap();

    let data = extract_data!(res).unwrap();
    assert_eq!(
        data,
        object! {
            parents: vec![
                object! {
                    children: vec![
                        object! {
                            foo: 1,
                            id: "c",
                        }
                    ]
                }
            ]
        }
    )
}

#[tokio::test]
async fn merge_fields_not_in_interface() {
    let subgraph_id = "MergeFieldsNotInInterface";
    let document = "interface Iface { id: ID! }
                  type Animal implements Iface @entity {
                    id: ID!
                    human: Iface!
                  }
                  type Human implements Iface @entity {
                    id: ID!
                    animal: Iface!
                  }
                  ";
    let schema = InputSchema::raw(document, subgraph_id);

    let query = "query {
                    ifaces {
                        ...on Animal {
                            id
                            friend: human {
                              id
                            }
                        }
                        ...on Human {
                            id
                            friend: animal {
                              id
                            }
                        }
                    }
            }";

    let animal = ("Animal", entity! { schema => id: "cow", human: "fred" });
    let human = ("Human", entity! { schema => id: "fred", animal: "cow" });

    let res = insert_and_query(subgraph_id, document, vec![animal, human], query)
        .await
        .unwrap();

    let data = extract_data!(res).unwrap();
    assert_eq!(
        data,
        object! {
            ifaces: vec![
                object! {
                    id: "cow",
                    friend: object! {
                        id: "fred",
                    },
                },
                object! {
                    id: "fred",
                    friend: object! {
                        id: "cow",
                    },
                },
            ]
        }
    )
}

#[tokio::test]
async fn nested_interface_fragments() {
    let subgraph_id = "NestedInterfaceFragments";
    let document = "interface I1face { id: ID!, foo1: Foo! }
                  interface I2face { id: ID!, foo2: Foo! }
                  interface I3face { id: ID!, foo3: Foo! }
                  type Foo @entity {
                      id: ID!
                  }
                  type One implements I1face @entity {
                    id: ID!
                    foo1: Foo!
                  }
                  type Two implements I1face & I2face @entity {
                    id: ID!
                    foo1: Foo!
                    foo2: Foo!
                  }
                  type Three implements I1face & I2face & I3face @entity {
                    id: ID!
                    foo1: Foo!
                    foo2: Foo!
                    foo3: Foo!
                  }";
    let schema = InputSchema::raw(document, subgraph_id);

    let query = "query {
                    i1Faces {
                        __typename
                        foo1 {
                            id
                        }
                        ...on I2face {
                            foo2 {
                                id
                            }
                        }
                        ...on I3face {
                            foo3 {
                                id
                            }
                        }
                    }
            }";

    let foo = ("Foo", entity! { schema => id: "foo" });
    let one = ("One", entity! { schema => id: "1", foo1: "foo" });
    let two = (
        "Two",
        entity! { schema => id: "2", foo1: "foo", foo2: "foo" },
    );
    let three = (
        "Three",
        entity! { schema => id: "3", foo1: "foo", foo2: "foo", foo3: "foo" },
    );

    let res = insert_and_query(subgraph_id, document, vec![foo, one, two, three], query)
        .await
        .unwrap();

    let data = extract_data!(res).unwrap();
    assert_eq!(
        data,
        object! {
            i1Faces: vec![
                object! {
                    __typename: "One",
                    foo1: object! {
                        id: "foo",
                    },
                },
                object! {
                    __typename: "Two",
                    foo1: object! {
                        id: "foo",
                    },
                    foo2: object! {
                        id: "foo",
                    },
                },
                object! {
                    __typename: "Three",
                    foo1: object! {
                        id: "foo",
                    },
                    foo2: object! {
                        id: "foo",
                    },
                    foo3: object! {
                        id: "foo",
                    },
                },
            ]
        }
    )
}

#[tokio::test]
async fn nested_interface_fragments_overlapping() {
    let subgraph_id = "NestedInterfaceFragmentsOverlapping";
    let document = "interface I1face { id: ID!, foo1: Foo! }
                  interface I2face { id: ID!, foo1: Foo! }
                  type Foo @entity {
                      id: ID!
                  }
                  type One implements I1face @entity {
                    id: ID!
                    foo1: Foo!
                  }
                  type Two implements I1face & I2face @entity {
                    id: ID!
                    foo1: Foo!
                  }";
    let schema = InputSchema::raw(document, subgraph_id);

    let query = "query {
                    i1Faces {
                        __typename
                        ...on I2face {
                            foo1 {
                                id
                            }
                        }
                    }
            }";

    let foo = ("Foo", entity! { schema => id: "foo" });
    let one = ("One", entity! { schema => id: "1", foo1: "foo" });
    let two = ("Two", entity! { schema => id: "2", foo1: "foo" });
    let res = insert_and_query(subgraph_id, document, vec![foo, one, two], query)
        .await
        .unwrap();

    let data = extract_data!(res).unwrap();
    assert_eq!(
        data,
        object! {
            i1Faces: vec![
                object! {
                    __typename: "One"
                },
                object! {
                    __typename: "Two",
                    foo1: object! {
                        id: "foo",
                    },
                },
            ]
        }
    );

    let query = "query {
        i1Faces {
            __typename
            foo1 {
                id
            }
            ...on I2face {
                foo1 {
                    id
                }
            }
        }
    }";

    let res = insert_and_query(subgraph_id, document, vec![], query)
        .await
        .unwrap();
    let data = extract_data!(res).unwrap();
    assert_eq!(
        data,
        object! {
            i1Faces: vec![
                object! {
                    __typename: "One",
                    foo1: object! {
                        id: "foo"
                    }
                },
                object! {
                    __typename: "Two",
                    foo1: object! {
                        id: "foo",
                    },
                },
            ]
        }
    );
}

#[tokio::test]
async fn enums() {
    use r::Value::Enum;
    let subgraph_id = "enums";
    let document = r#"
       enum Direction {
         NORTH
         EAST
         SOUTH
         WEST
       }

       type Trajectory @entity {
         id: ID!
         direction: Direction!
         meters: Int!
       }"#;
    let schema = InputSchema::raw(document, subgraph_id);

    let entities = vec![
        (
            "Trajectory",
            entity! { schema =>  id: "1", direction: "EAST", meters: 10 },
        ),
        (
            "Trajectory",
            entity! { schema =>  id: "2", direction: "NORTH", meters: 15 },
        ),
    ];
    let query = "query { trajectories { id, direction, meters } }";

    let res = insert_and_query(subgraph_id, document, entities, query)
        .await
        .unwrap();
    let data = extract_data!(res).unwrap();
    assert_eq!(
        data,
        object! {
        trajectories: vec![
            object!{
        id: "1",
        direction: Enum("EAST".to_string()),
        meters: 10,
        },
            object!{
                id: "2",
        direction: Enum("NORTH".to_string()),
        meters: 15,
            },
        ]}
    );
}

#[tokio::test]
async fn enum_list_filters() {
    use r::Value::Enum;
    let subgraph_id = "enum_list_filters";
    let document = r#"
       enum Direction {
         NORTH
         EAST
         SOUTH
         WEST
       }

       type Trajectory @entity {
         id: ID!
         direction: Direction!
         meters: Int!
       }"#;
    let schema = InputSchema::raw(document, subgraph_id);

    let entities = vec![
        (
            "Trajectory",
            entity! { schema =>  id: "1", direction: "EAST", meters: 10 },
        ),
        (
            "Trajectory",
            entity! { schema =>  id: "2", direction: "NORTH", meters: 15 },
        ),
        (
            "Trajectory",
            entity! { schema =>  id: "3", direction: "WEST", meters: 20 },
        ),
    ];

    let query = "query { trajectories(where: { direction_in: [NORTH, EAST] }) { id, direction } }";
    let res = insert_and_query(subgraph_id, document, entities, query)
        .await
        .unwrap();
    let data = extract_data!(res).unwrap();
    assert_eq!(
        data,
        object! {
        trajectories: vec![
            object!{
                id: "1",
                direction: Enum("EAST".to_string()),
            },
            object!{
                id: "2",
                direction: Enum("NORTH".to_string()),
            },
        ]}
    );

    let query = "query { trajectories(where: { direction_not_in: [EAST] }) { id, direction } }";
    let res = insert_and_query(subgraph_id, document, vec![], query)
        .await
        .unwrap();
    let data = extract_data!(res).unwrap();
    assert_eq!(
        data,
        object! {
        trajectories: vec![
            object!{
                id: "2",
                direction: Enum("NORTH".to_string()),
            },
            object!{
                id: "3",
                direction: Enum("WEST".to_string()),
            },
        ]}
    );
}

#[tokio::test]
async fn recursive_fragment() {
    // Depending on whether `ENABLE_GRAPHQL_VALIDATIONS` is set or not, we
    // get different error messages
    const FOO_ERRORS: [&str; 2] = [
        "Cannot spread fragment \"FooFrag\" within itself.",
        "query has fragment cycle including `FooFrag`",
    ];
    const FOO_BAR_ERRORS: [&str; 2] = [
        "Cannot spread fragment \"BarFrag\" within itself via \"FooFrag\".",
        "query has fragment cycle including `BarFrag`",
    ];
    let subgraph_id = "RecursiveFragment";
    let schema = "
        type Foo @entity {
            id: ID!
            foo: Foo!
            bar: Bar!
        }

        type Bar @entity {
            id: ID!
            foo: Foo!
        }
    ";

    let self_recursive = "
        query {
            foos {
              ...FooFrag
            }
        }

        fragment FooFrag on Foo {
          id
          foo {
            ...FooFrag
          }
        }
    ";
    let res = insert_and_query(subgraph_id, schema, vec![], self_recursive)
        .await
        .unwrap();
    let data = res.to_result().unwrap_err()[0].to_string();
    assert!(FOO_ERRORS.contains(&data.as_str()));

    let co_recursive = "
        query {
          foos {
            ...BarFrag
          }
        }

        fragment BarFrag on Bar {
          id
          foo {
             ...FooFrag
          }
        }

        fragment FooFrag on Foo {
          id
          bar {
            ...BarFrag
          }
        }
    ";
    let res = insert_and_query(subgraph_id, schema, vec![], co_recursive)
        .await
        .unwrap();
    let data = res.to_result().unwrap_err()[0].to_string();
    assert!(FOO_BAR_ERRORS.contains(&data.as_str()));
}

#[tokio::test]
async fn mixed_mutability() {
    let subgraph_id = "MixedMutability";
    let document = "interface Event { id: String! }
                  type Mutable implements Event @entity { id: String!, name: String! }
                  type Immutable implements Event @entity(immutable: true) { id: String!, name: String! }";
    let schema = InputSchema::raw(document, subgraph_id);

    let query = "query { events { id } }";

    let entities = vec![
        ("Mutable", entity! { schema =>  id: "mut0", name: "mut0" }),
        (
            "Immutable",
            entity! { schema =>  id: "immo0", name: "immo0" },
        ),
    ];

    {
        // We need to remove the subgraph since it contains immutable
        // entities, and the trick the other tests use does not work for
        // this. They rely on the EntityCache filtering out entity changes
        // that are already in the store
        let id = DeploymentHash::new(subgraph_id).unwrap();
        remove_subgraph(&id);
    }
    let res = insert_and_query(subgraph_id, document, entities, query)
        .await
        .unwrap();

    let data = extract_data!(res).unwrap();
    let exp = object! { events: vec![ object!{ id: "immo0" }, object! { id: "mut0" } ] };
    assert_eq!(data, exp);
}

#[tokio::test]
async fn derived_interface_bytes() {
    fn b(s: &str) -> Value {
        Value::Bytes(s.parse().unwrap())
    }

    let subgraph_id = "DerivedInterfaceBytes";
    let document = r#" type Pool @entity {
        id: Bytes!,
        trades: [Trade!]! @derivedFrom(field: "pool")
      }

      interface Trade {
       id: Bytes!
       pool: Pool!
      }

      type Sell implements Trade @entity {
          id: Bytes!
          pool: Pool!
      }
      type Buy implements Trade @entity {
       id: Bytes!
       pool: Pool!
      }"#;
    let schema = InputSchema::raw(document, subgraph_id);

    let query = "query { pools { trades { id } } }";

    let entities = vec![
        ("Pool", entity! { schema =>  id: b("0xf001") }),
        ("Sell", entity! { schema =>  id: b("0xc0"), pool: "0xf001"}),
        ("Buy", entity! { schema =>  id: b("0xb0"), pool: "0xf001"}),
    ];

    let res = insert_and_query(subgraph_id, document, entities, query)
        .await
        .unwrap();

    let data = extract_data!(res).unwrap();
    let exp = object! { pools: vec![ object!{ trades: vec![ object! { id: "0xb0" }, object! { id: "0xc0" }] } ] };
    assert_eq!(data, exp);
}
