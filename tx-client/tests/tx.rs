use tx_client::client::TxClient;

use actix_web::{test, App};
use fuel_core::service;

use fuel_vm::consts::*;
use fuel_vm::prelude::*;

#[actix_rt::test]
async fn transact() {
    let srv = test::start(|| App::new().configure(service::configure));
    let client = TxClient::from(srv.addr());

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
    assert_eq!(3, log.len());

    assert!(matches!(log[0],
        LogEvent::Register {
            register, value, ..
        } if register == 0x10 && value == 0xca));

    assert!(matches!(log[1],
        LogEvent::Register {
            register, value, ..
        } if register == 0x11 && value == 0xba));

    assert!(matches!(log[2],
        LogEvent::Return {
            register, value, ..
        } if register == REG_ONE && value == 1));
}
