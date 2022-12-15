#[cfg(test)]
mod tests {
    use client_14::{
        client::FuelClient as FC14,
        prelude::Opcode,
    };
    use client_15::client::{
        FuelClient as FC15,
        PageDirection,
        PaginationRequest,
    };
    use core_14::service::{
        Config as Config14,
        DbType as DbType14,
        FuelService as FS14,
    };
    use core_15::service::{
        Config as Config15,
        DbType as DbType15,
        FuelService as FS15,
    };
    use fuel_tx_old::{
        Input,
        Output,
        Transaction,
        TransactionBuilder,
    };
    use std::{
        ops::Deref,
        time::Duration,
    };
    use tempdir::TempDir;
    use tokio::time;

    fn tx(i: u64) -> Transaction {
        let predicate: Vec<u8> = [Opcode::RET(0x01)].into_iter().collect();
        let owner = Input::predicate_owner(&predicate);

        TransactionBuilder::script(predicate.clone(), vec![])
            .add_input(Input::coin_predicate(
                Default::default(),
                owner,
                500,
                Default::default(),
                Default::default(),
                0,
                predicate,
                vec![],
            ))
            .add_output(Output::Coin {
                to: Default::default(),
                amount: 250,
                asset_id: Default::default(),
            })
            .gas_limit(10000 + i)
            .finalize_as_transaction()
    }

    #[tokio::test]
    async fn can_read_older_blocks() {
        let tmp_dir = TempDir::new("fuel_core_version_tests").unwrap();
        let db_path = tmp_dir.path().to_path_buf();

        let old_config = Config14 {
            database_type: DbType14::RocksDb,
            database_path: db_path.clone(),
            manual_blocks_enabled: true,
            ..Config14::local_node()
        };
        let service_1 = FS14::new_node(old_config).await.unwrap();
        let service_1_client = FC14::from(service_1.bound_address);

        // trigger some block production
        let mut tx_ids = vec![];
        tx_ids.push(service_1_client.submit(&tx(0)).await.unwrap());
        tx_ids.push(service_1_client.submit(&tx(1)).await.unwrap());
        tx_ids.push(service_1_client.submit(&tx(2)).await.unwrap());
        tx_ids.push(service_1_client.submit(&tx(3)).await.unwrap());
        tx_ids.push(service_1_client.submit(&tx(4)).await.unwrap());
        tx_ids.push(service_1_client.submit(&tx(5)).await.unwrap());

        // wait for txs to process and client to shutdown
        time::sleep(Duration::from_secs(1)).await;
        service_1.stop().await;
        time::sleep(Duration::from_secs(1)).await;

        // start v0.15 client using the same db as v0.14
        let new_config = Config15 {
            database_type: DbType15::RocksDb,
            database_path: db_path,
            manual_blocks_enabled: true,
            ..Config15::local_node()
        };
        let service_2 = FS15::new_node(new_config).await.unwrap();
        let service_2_client = FC15::from(service_2.bound_address);
        // fetch blocks produced by older node
        let blocks = service_2_client
            .blocks(PaginationRequest {
                cursor: None,
                results: 20,
                direction: PageDirection::Backward,
            })
            .await
            .unwrap();

        let ret_tx_ids = blocks
            .results
            .iter()
            .flat_map(|b| b.transactions.iter().map(|tx| tx.id.0 .0.deref().clone()))
            .collect::<Vec<_>>();

        let is_any_tx_missing = tx_ids
            .iter()
            .map(|id| id.0 .0.deref())
            .any(|id| !ret_tx_ids.contains(&id));
        assert!(!is_any_tx_missing);
    }
}
