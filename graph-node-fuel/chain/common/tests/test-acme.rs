const PROTO_FILE: &str = "tests/resources/acme.proto";

use graph_chain_common::*;

#[test]
fn check_repeated_type_ok() {
    let types = parse_proto_file(PROTO_FILE).expect("Unable to read proto file!");

    let array_types = types
        .iter()
        .flat_map(|(_, t)| t.fields.iter())
        .filter(|t| t.is_array)
        .map(|t| t.type_name.clone())
        .collect::<std::collections::HashSet<_>>();

    let mut array_types = array_types.into_iter().collect::<Vec<String>>();
    array_types.sort();

    assert_eq!(3, array_types.len());
    assert_eq!(array_types, vec!["Attribute", "Event", "Transaction"]);
}

#[test]
fn check_type_count_ok() {
    let types = parse_proto_file(PROTO_FILE).expect("Unable to read proto file!");
    assert_eq!(7, types.len());
}

#[test]
fn required_ok() {
    let types = parse_proto_file(PROTO_FILE).expect("Unable to read proto file!");
    let msg = types.get("Transaction");
    assert!(msg.is_some(), "\"Transaction\" type is not available!");

    let ptype = msg.unwrap();
    assert_eq!(8, ptype.fields.len());

    ptype.fields.iter().for_each(|f| {
        match f.name.as_ref() {
            "type" => assert!(f.required, "Transaction.type field should be required!"),
            "hash" => assert!(
                !f.required,
                "Transaction.hash field should NOT be required!"
            ),
            "sender" => assert!(
                !f.required,
                "Transaction.sender field should NOT be required!"
            ),
            "receiver" => assert!(
                !f.required,
                "Transaction.receiver field should NOT be required!"
            ),
            "amount" => assert!(
                !f.required,
                "Transaction.amount field should NOT be required!"
            ),
            "fee" => assert!(!f.required, "Transaction.fee field should NOT be required!"),
            "success" => assert!(
                !f.required,
                "Transaction.success field should NOT be required!"
            ),
            "events" => assert!(
                !f.required,
                "Transaction.events field should NOT be required!"
            ),
            _ => assert!(false, "Unexpected message field [{}]!", f.name),
        };
    });
}

#[test]
fn enum_ok() {
    let types = parse_proto_file(PROTO_FILE).expect("Unable to read proto file!");
    let msg = types.get("EnumTest");
    assert!(msg.is_some(), "\"EnumTest\" type is not available!");

    let ptype = msg.unwrap();
    assert_eq!(1, ptype.fields.len());
}

#[test]
fn enum_mixed_ok() {
    let types = parse_proto_file(PROTO_FILE).expect("Unable to read proto file!");
    let msg = types.get("MixedEnumTest");
    assert!(msg.is_some(), "\"MixedEnumTest\" type is not available!");

    let ptype = msg.unwrap();
    assert_eq!(2, ptype.fields.len());
}
