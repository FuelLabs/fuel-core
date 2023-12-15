use graph::data::value::Object;
use graph::data::{graphql::object, query::QueryResults};
use graph::prelude::*;
use graph_server_http::test_utils;

#[test]
fn generates_200_for_query_results() {
    let data = Object::from_iter([]);
    let query_result = QueryResults::from(data).as_http_response();
    test_utils::assert_expected_headers(&query_result);
    test_utils::assert_successful_response(query_result);
}

#[test]
fn generates_valid_json_for_an_empty_result() {
    let data = Object::from_iter([]);
    let query_result = QueryResults::from(data).as_http_response();
    test_utils::assert_expected_headers(&query_result);
    let data = test_utils::assert_successful_response(query_result);
    assert!(data.is_empty());
}

#[test]
fn canonical_serialization() {
    macro_rules! assert_resp {
        ($exp: expr, $obj: expr) => {{
            {
                // This match is solely there to make sure these tests
                // get amended if q::Value ever gets more variants
                // The order of the variants should be the same as the
                // order of the tests below
                use r::Value::*;
                let _ = match $obj {
                    Object(_) | List(_) | Enum(_) | Null | Int(_) | Float(_) | String(_)
                    | Boolean(_) => (),
                };
            }
            let res = QueryResult::try_from($obj).unwrap();
            assert_eq!($exp, serde_json::to_string(&res).unwrap());
        }};
    }
    assert_resp!(r#"{"data":{"id":"12345"}}"#, object! { id: "12345" });

    // Value::Variable: nothing to check, not used in a response

    // Value::Object: Insertion order of keys matters
    let first_second = r#"{"data":{"first":"first","second":"second"}}"#;
    let second_first = r#"{"data":{"second":"second","first":"first"}}"#;
    assert_resp!(first_second, object! { first: "first", second: "second" });
    assert_resp!(second_first, object! { second: "second", first: "first" });

    // Value::List
    assert_resp!(r#"{"data":{"ary":[1,2]}}"#, object! { ary: vec![1,2] });

    // Value::Enum
    assert_resp!(
        r#"{"data":{"enum_field":"enum"}}"#,
        object! { enum_field:  r::Value::Enum("enum".to_owned())}
    );

    // Value::Null
    assert_resp!(
        r#"{"data":{"nothing":null}}"#,
        object! { nothing:  r::Value::Null}
    );

    // Value::Int
    assert_resp!(
        r#"{"data":{"int32":17,"neg32":-314}}"#,
        object! { int32: 17, neg32: -314 }
    );

    // Value::Float
    assert_resp!(r#"{"data":{"float":3.14159}}"#, object! { float: 3.14159 });

    // Value::String
    assert_resp!(
        r#"{"data":{"text":"Ünïcödë with spaceß"}}"#,
        object! { text: "Ünïcödë with spaceß" }
    );

    // Value::Boolean
    assert_resp!(
        r#"{"data":{"no":false,"yes":true}}"#,
        object! { no: false, yes: true }
    );
}
