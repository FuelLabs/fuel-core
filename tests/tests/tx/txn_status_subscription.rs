use std::time::Duration;

use fuel_core::service::{
    Config,
    FuelService,
};
use fuel_core_client::client::FuelClient;
use fuel_core_types::{
    fuel_asm::*,
    fuel_tx::*,
};
use futures::StreamExt;

#[tokio::test]
async fn subscribe_txn_status() {
    let mut config = Config::local_node();
    config.block_production = fuel_core::service::config::Trigger::Interval {
        block_time: Duration::from_secs(2),
    };
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let gas_price = 10;
    let gas_limit = 1_000_000;
    let maturity = Default::default();

    let create_script = |i: u8| {
        // The first two scripts will run and the rest will fail.
        let script = vec![op::addi(0x11 - i, 0x10, 1), op::ret(RegId::ONE)];
        let script: Vec<u8> = script
            .iter()
            .flat_map(|op| u32::from(*op).to_be_bytes())
            .collect();

        let predicate = op::ret(RegId::ONE).to_bytes().to_vec();
        let owner = Input::predicate_owner(&predicate);
        // The third transaction needs to have a different input.
        let utxo_id = if i == 2 { 2 } else { 1 };
        let utxo_id = UtxoId::new(Bytes32::from([utxo_id; 32]), 1);
        let coin_input = Input::coin_predicate(
            utxo_id,
            owner,
            1000,
            AssetId::zeroed(),
            TxPointer::default(),
            Default::default(),
            predicate,
            vec![],
        );
        let tx: Transaction = Transaction::script(
            gas_price + (i as u64),
            gas_limit,
            maturity,
            script,
            vec![],
            vec![coin_input],
            vec![],
            vec![],
        )
        .into();
        tx
    };
    let txns: Vec<_> = (0..3).map(create_script).collect();
    let mut jhs = vec![];

    for (txn_idx, id) in txns.iter().map(|t| t.id().to_string()).enumerate() {
        let jh = tokio::spawn({
            let client = client.clone();
            async move {
                client
                    .subscribe_transaction_status(&id)
                    .await
                    .unwrap()
                    .enumerate()
                    .for_each(|(event_idx, r)| async move {
                        let r = r.unwrap();
                        match (txn_idx, event_idx) {
                            (0, 0) => assert!(matches!(r, fuel_core_client::client::types::TransactionStatus::Submitted{ .. }), "{r:?}"),
                            (0, 1) => assert!(matches!(r, fuel_core_client::client::types::TransactionStatus::SqueezedOut{ .. }), "{r:?}"),
                            (1, 0) => assert!(matches!(r, fuel_core_client::client::types::TransactionStatus::Submitted{ .. }), "{r:?}"),
                            (1, 1) => assert!(matches!(r, fuel_core_client::client::types::TransactionStatus::Success{ .. }), "{r:?}"),
                            (2, 0) => assert!(matches!(r, fuel_core_client::client::types::TransactionStatus::Submitted{ .. }), "{r:?}"),
                            (2, 1) => assert!(matches!(r, fuel_core_client::client::types::TransactionStatus::Failure{ .. }), "{r:?}"),
                            _ => unreachable!("{} {} {:?}", txn_idx, event_idx, r),
                        }
                    })
                    .await;
            }
        });
        jhs.push(jh);
    }

    for tx in &txns {
        client.submit(tx).await.unwrap();
    }

    for jh in jhs {
        jh.await.unwrap();
    }
}
