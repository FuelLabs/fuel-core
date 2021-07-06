use dap_client::client::DapClient;

use actix_web::{test, App};
use fuel_core::service;
use std::convert::TryInto;

use fuel_vm::consts::*;
use fuel_vm::prelude::*;

#[actix_rt::test]
async fn start_session() {
    let srv = test::start(|| App::new().configure(service::configure));
    let client = DapClient::from(srv.addr());

    let session = client.start_session().await.unwrap();
    let session_p = client.start_session().await.unwrap();

    let id = session.as_str();
    let id_p = session_p.as_str();

    assert_ne!(id, id_p);
}

#[actix_rt::test]
async fn end_session() {
    let srv = test::start(|| App::new().configure(service::configure));
    let client = DapClient::from(srv.addr());

    let session = client.start_session().await.unwrap();
    let id = session.as_str();

    assert!(client.end_session(id).await.unwrap());
    assert!(!client.end_session(id).await.unwrap());
}

#[actix_rt::test]
async fn reset() {
    let srv = test::start(|| App::new().configure(service::configure));
    let client = DapClient::from(srv.addr());

    let session = client.start_session().await.unwrap();
    let id = session.as_str();

    let register = client.register(id, 0x10).await.unwrap();
    assert_eq!(0x00, register);

    let result = client
        .execute(id, &Opcode::ADDI(0x10, 0x10, 0xfa))
        .await
        .unwrap();
    assert!(result);

    let register = client.register(id, 0x10).await.unwrap();
    assert_eq!(0xfa, register);

    let result = client
        .execute(id, &Opcode::ADDI(0x11, 0x11, 0x08))
        .await
        .unwrap();
    assert!(result);

    let result = client.execute(id, &Opcode::ALOC(0x11)).await.unwrap();
    assert!(result);

    let result = client
        .execute(id, &Opcode::ADDI(0x11, REG_HP, 1))
        .await
        .unwrap();
    assert!(result);

    let result = client
        .execute(id, &Opcode::SW(0x11, 0x10, 0))
        .await
        .unwrap();
    assert!(result);

    let memory = client.register(id, 0x11).await.unwrap();
    let memory = client.memory(id, memory as RegisterId, 8).await.unwrap();
    let memory = Word::from_be_bytes(memory.as_slice().try_into().unwrap());
    assert_eq!(0xfa, memory);

    let result = client.reset(id).await.unwrap();
    assert!(result);

    let register = client.register(id, 0x10).await.unwrap();
    assert_eq!(0x00, register);

    let result = client.end_session(id).await.unwrap();
    assert!(result);
}
