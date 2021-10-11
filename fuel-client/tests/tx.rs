use fuel_client::client::FuelClient;
use fuel_core::service::{configure, run_in_background};
use fuel_vm::consts::*;
use fuel_vm::prelude::*;

#[tokio::test]
async fn transact() {
    let srv = run_in_background(configure(Default::default())).await;
    let client = FuelClient::from(srv);

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
        .map(|op| u32::from(*op).to_be_bytes())
        .flatten()
        .collect();

    let tx = Transaction::script(
        gas_price,
        gas_limit,
        maturity,
        script,
        vec![],
        vec![],
        vec![],
        vec![],
    );

    let log = client.transact(&tx).await.unwrap();
    assert_eq!(2, log.len());

    assert!(matches!(log[0],
        Receipt::Log {
            ra, rb, ..
        } if ra == 0xca && rb == 0xba));

    assert!(matches!(log[1],
        Receipt::Return {
            val, ..
        } if val == 1));
}
