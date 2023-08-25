#[cfg(test)]
mod test {
    use f_core::{
        database::Database,
        service::{
            Config,
            FuelService,
        },
    };
    use fuel_gql_client::{
        client::{
            types::TransactionStatus,
            FuelClient,
        },
        fuel_tx,
        fuel_tx::UniqueIdentifier,
    };

    // `cynic/http-reqwest` from client can work with old version of `fuel-core`
    #[tokio::test]
    async fn submit_tx() {
        let db = Database::default();

        let config = Config::local_node();

        let srv = FuelService::from_database(db, config.clone())
            .await
            .unwrap();

        let client = FuelClient::from(srv.bound_address);

        let tx = fuel_tx::Transaction::default();
        client.submit_and_await_commit(&tx).await.unwrap();

        let transaction_response = client
            .transaction(&format!("{:#x}", tx.id()))
            .await
            .unwrap();

        assert!(transaction_response.is_some());
    }

    #[tokio::test]
    async fn produce_block_compatible() {
        let db = Database::default();

        let config = Config::local_node();

        let srv = FuelService::from_database(db, config.clone())
            .await
            .unwrap();

        let client = FuelClient::from(srv.bound_address);

        let new_height = client.produce_blocks(5, None).await.unwrap();

        assert_eq!(5, new_height);

        let tx = fuel_tx::Transaction::default();
        client.submit_and_await_commit(&tx).await.unwrap();

        let transaction_response = client
            .transaction(&format!("{:#x}", tx.id()))
            .await
            .unwrap();

        if let TransactionStatus::Success { block_id, .. } =
            transaction_response.unwrap().status
        {
            let block = client.block(block_id.to_string().as_str()).await;
            assert!(block.is_err());
        } else {
            panic!("Wrong tx status");
        };
    }
}
