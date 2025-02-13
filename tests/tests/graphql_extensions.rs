use fuel_core::service::Config;
use fuel_core_bin::FuelService;
use serde_json::Value;
use test_helpers::send_graph_ql_query;

use test_case::test_case;

// TODO[RC]: Provide more detailed tests to verify that the correct STF and CP versions
// are returned.

#[test_case("current_stf_version")]
#[test_case("current_fuel_block_height")]
#[test_case("current_consensus_parameters_version")]
#[tokio::test]
async fn current_stf_version_is_present(extension_name: &str) {
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
    let is_field_present = extensions.get(extension_name).is_some();
    assert!(is_field_present)
}
