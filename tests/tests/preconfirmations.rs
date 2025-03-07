use std::time::Duration;

use fuel_core::service::Config;
use fuel_core_bin::FuelService;
use fuel_core_client::client::{
    types::TransactionStatus,
    FuelClient,
};
use fuel_core_poa::Trigger;
use fuel_core_types::{
    fuel_asm::{
        op,
        RegId,
    },
    fuel_tx::{
        Address,
        AssetId,
        Output,
        Receipt,
        TransactionBuilder,
        TxPointer,
    },
    fuel_types::BlockHeight,
};
use futures::StreamExt;

#[tokio::test]
async fn preconfirmation__received_after_execution() {
    let mut config = Config::local_node();
    let block_production_period = Duration::from_secs(1);
    let tip = 2;
    let address = Address::new([0; 32]);

    config.block_production = Trigger::Open {
        period: block_production_period,
    };
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let gas_limit = 1_000_000;
    let maturity = Default::default();

    let script = [
        op::addi(0x10, RegId::ZERO, 0xca),
        op::addi(0x11, RegId::ZERO, 0xba),
        op::log(0x10, 0x11, RegId::ZERO, RegId::ZERO),
        op::ret(RegId::ONE),
    ];
    let script: Vec<u8> = script
        .iter()
        .flat_map(|op| u32::from(*op).to_be_bytes())
        .collect();

    let tx = TransactionBuilder::script(script, vec![])
        .script_gas_limit(gas_limit)
        .tip(tip)
        .maturity(maturity)
        .add_fee_input()
        .add_output(Output::variable(address, 0, AssetId::default()))
        .finalize_as_transaction();

    let tx_id = client.submit(&tx).await.unwrap();
    let mut tx_statuses_subscriber =
        client.subscribe_transaction_status(&tx_id).await.unwrap();

    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Submitted { .. }
    ));

    if let TransactionStatus::PreconfirmationSuccess {
        tx_pointer,
        total_fee,
        total_gas,
        transaction_id,
        receipts,
        outputs,
    } = tx_statuses_subscriber.next().await.unwrap().unwrap()
    {
        assert_eq!(tx_pointer, TxPointer::new(BlockHeight::new(1), 0));
        assert_eq!(total_fee, tip);
        assert_eq!(total_gas, 4450);
        assert_eq!(transaction_id, tx_id);
        let receipts = receipts.unwrap();
        assert_eq!(receipts.len(), 2);
        assert!(matches!(receipts[0],
            Receipt::Log {
                ra, rb, ..
            } if ra == 0xca && rb == 0xba));

        assert!(matches!(receipts[1],
            Receipt::Return {
                val, ..
            } if val == 1));
        let outputs = outputs.unwrap();
        assert_eq!(outputs.len(), 1);
        assert_eq!(
            outputs[0],
            Output::Coin {
                to: address,
                amount: 2,
                asset_id: AssetId::default()
            }
        );
    } else {
        panic!("Expected preconfirmation status");
    }

    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Success { .. }
    ));
}

#[tokio::test]
async fn preconfirmation__received_after_failed_execution() {}

#[tokio::test]
async fn preconfirmation__received_after_squeezed_out() {}

#[tokio::test]
async fn preconfirmation__received_tx_inserted_end_block_open_period() {}

#[tokio::test]
async fn preconfirmation__received_after_execution__multiple_txs() {}

#[tokio::test]
async fn preconfirmation__propagate_p2p_after_execution() {}

#[tokio::test]
async fn preconfirmation__propagate_p2p_after_failed_execution() {}

#[tokio::test]
async fn preconfirmation__propagate_p2p_after_squeezed_out() {}
