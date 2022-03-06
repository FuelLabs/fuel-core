use anyhow::Result;
use fuel_gql_client::client::FuelClient;
use fuel_vm::{consts::*, prelude::*};
use std::net::SocketAddr;

fn create_log_transaction(rega: u16, regb: u16) -> Transaction {
    let script = vec![
        Opcode::ADDI(0x10, REG_ZERO, rega),
        Opcode::ADDI(0x11, REG_ZERO, regb),
        Opcode::LOG(0x10, 0x11, REG_ZERO, REG_ZERO),
        Opcode::RET(REG_ONE),
    ]
    .into_iter()
    .collect::<Vec<u8>>();

    let gas_price = 0;
    let gas_limit = 1_000_000;
    let maturity = 0;
    Transaction::script(
        gas_price,
        gas_limit,
        maturity,
        script,
        vec![],
        vec![],
        vec![],
        vec![],
    )
}

#[tokio::main]
pub async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let addr: SocketAddr = args[1].parse().expect("Bad addr");
    let client = FuelClient::from(addr);
    let result = client.submit(&create_log_transaction(0x0a, 0x02)).await;
    println!("Result 0 {result:?}");
    let result = client.submit(&create_log_transaction(0xfa, 0x03)).await;
    println!("Result 1 {result:?}");
    let result = client.submit(&create_log_transaction(0x0a, 0x03)).await;
    println!("Result 2 {result:?}");
    let result = client.submit(&create_log_transaction(0xfa, 0x04)).await;
    println!("Result 1 {result:?}");

    Ok(())
}
