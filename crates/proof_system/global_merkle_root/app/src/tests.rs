#![allow(non_snake_case)]

use fuel_core::{
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::FuelClient;

use crate::{
    app::App,
    cli,
};

#[tokio::test]
async fn app__returns_state_root_when_queried() {
    // Given
    let fuel_service =
        FuelService::from_database(Database::default(), Config::local_node())
            .await
            .unwrap();
    let client = FuelClient::from(fuel_service.bound_address);
    let fuel_node_url = format!("http://{}/v1/graphql", fuel_service.bound_address);

    let app_args = cli::Args {
        fuel_node_url,
        ..cli::Args::test_defaults()
    };

    let app = App::new(&app_args).await.unwrap();
    // When
    app.start().await.unwrap();
    client.produce_blocks(5, None).await.unwrap();
    // Then
    todo!();
}
