use fuel_core::service::Config;
use fuel_core_bin::FuelService;
use serde_json::Value;
use test_helpers::send_graph_ql_query;

// TODO[RC]: Provide more detailed tests to verify that the correct STF and CP versions
// are returned.

#[tokio::test]
async fn extension_fields_are_present() {
    const REQUIRED_FIELDS: [&str; 3] = [
        "current_stf_version",
        "current_fuel_block_height",
        "current_consensus_parameters_version",
    ];

    let node = FuelService::new_node(Config::local_node()).await.unwrap();
    let url = format!("http://{}/v1/graphql", node.bound_address);

    // Given
    const QUERY: &str = r#"
        query {
          nodeInfo {
            nodeVersion
          }
        }
    "#;

    // When
    let response = send_graph_ql_query(&url, QUERY).await;

    // Then
    let json_value: Value =
        serde_json::from_str(&response).expect("should be valid json");
    let extensions = json_value
        .get("extensions")
        .expect("should have extensions");
    for field in REQUIRED_FIELDS.iter() {
        let is_field_present = extensions.get(field).is_some();
        assert!(is_field_present)
    }
}
