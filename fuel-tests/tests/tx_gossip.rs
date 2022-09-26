use crate::helpers::TestContext;
use chrono::Utc;
use fuel_core::{
    database::Database,
    executor::Executor,
    model::{
        FuelBlock,
        FuelBlockHeader,
    },
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_interfaces::{
    common::{
        fuel_tx,
        fuel_vm::{
            consts::*,
            prelude::*,
        },
    },
    executor::ExecutionMode,
};
use fuel_gql_client::client::{
    types::TransactionStatus,
    FuelClient,
    PageDirection,
    PaginationRequest,
};

#[tokio::test]
async fn test_tx_gossiping() {
    let node_config = Config::local_node();
    let node_one = FuelService::new_node(node_config.clone()).await.unwrap();
    let client_one = FuelClient::from(node_one.bound_address);

    let node_two = FuelService::new_node(node_config).await.unwrap();
    let client_two = FuelClient::from(node_two.bound_address);

    let gas_price = 0;
    let gas_limit = 1_000_000;
    let maturity = 0;

    let script = vec![
        Opcode::ADDI(0x10, REG_ZERO, 0xca),
        Opcode::ADDI(0x11, REG_ZERO, 0xba),
        Opcode::LOG(0x10, 0x11, REG_ZERO, REG_ZERO),
        Opcode::RET(REG_ONE),
    ];
    let script: Vec<u8> = script
        .iter()
        .flat_map(|op| u32::from(*op).to_be_bytes())
        .collect();

    let tx = fuel_tx::Transaction::script(
        gas_price,
        gas_limit,
        maturity,
        script,
        vec![],
        vec![],
        vec![],
        vec![],
    );

    let result = client_one.submit(&tx).await.unwrap();

    // Perhaps some delay is needed before this query?

    let tx = client_two.transaction(&result.0.to_string()).await.unwrap();

    assert!(tx.is_some());
}
