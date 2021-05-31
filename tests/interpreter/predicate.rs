use super::common::r;
use fuel_core::consts::*;
use fuel_core::prelude::*;
use fuel_tx::bytes;

#[test]
fn predicate() {
    let mut vm = Interpreter::default();

    let predicate_data = 0x23 as Word;
    let mut predicate = vec![];

    predicate.push(Opcode::ADDI(0x10, REG_ZERO, 0x11));
    predicate.push(Opcode::ADDI(0x11, 0x10, 0x12));
    predicate.push(Opcode::ADDI(0x12, REG_ZERO, 0x08));
    predicate.push(Opcode::ALOC(0x12));
    predicate.push(Opcode::ADDI(0x12, REG_HP, 0x01));
    predicate.push(Opcode::SW(0x12, 0x11, 0));

    let predicate = predicate
        .into_iter()
        .map(|op| u32::from(op).to_be_bytes())
        .flatten()
        .collect();

    let predicate_data = predicate_data.to_be_bytes().to_vec();

    let maturity = 100;
    let salt: Salt = r();
    let witness = vec![];
    let contract = Contract::from(witness.as_slice());
    let contract = contract.address(&salt);

    let input = Input::coin(r(), r(), 0, r(), 0, maturity, predicate, predicate_data);
    let output = Output::contract_created(contract);

    let gas_price = 10;
    let gas_limit = 1_000_000;
    let bytecode_witness_index = 0;
    let static_contracts = vec![];
    let inputs = vec![input];
    let outputs = vec![output];
    let witnesses = vec![witness.into()];

    let mut tx = Transaction::create(
        gas_price,
        gas_limit,
        maturity,
        bytecode_witness_index,
        salt,
        static_contracts,
        inputs,
        outputs,
        witnesses,
    );

    let predicate_offset = tx.input_coin_predicate_offset(0).expect("Inconsistent inputs!");
    match tx.inputs_mut().get_mut(0) {
        Some(Input::Coin { predicate, .. }) => {
            let mut p = vec![];

            let predicate_data_offset =
                Interpreter::tx_mem_address() + predicate_offset + bytes::padded_len(predicate.as_slice()) + 16; // 4 additional opcodes represented as u32
            p.push(Opcode::ADDI(0x10, REG_ZERO, 0x08));
            p.push(Opcode::ADDI(0x11, REG_ZERO, predicate_data_offset as Immediate12));
            p.push(Opcode::MEQ(0x10, 0x11, 0x12, 0x10));
            p.push(Opcode::RET(0x10));

            let p: Vec<u8> = p.into_iter().map(|op| u32::from(op).to_be_bytes()).flatten().collect();

            predicate.extend_from_slice(p.as_slice());
        }
        _ => panic!("Inconsistent transaction!"),
    }

    vm.init(tx).expect("Failed to initialize VM");
    vm.run().expect("Failed to execute predicate");
}

#[test]
fn predicate_false() {
    let mut vm = Interpreter::default();

    let predicate_data = 0x24 as Word;
    let mut predicate = vec![];

    predicate.push(Opcode::ADDI(0x10, REG_ZERO, 0x11));
    predicate.push(Opcode::ADDI(0x11, 0x10, 0x12));
    predicate.push(Opcode::ADDI(0x12, REG_ZERO, 0x08));
    predicate.push(Opcode::ALOC(0x12));
    predicate.push(Opcode::ADDI(0x12, REG_HP, 0x01));
    predicate.push(Opcode::SW(0x12, 0x11, 0));

    let predicate = predicate
        .into_iter()
        .map(|op| u32::from(op).to_be_bytes())
        .flatten()
        .collect();

    let predicate_data = predicate_data.to_be_bytes().to_vec();

    let maturity = 100;
    let salt: Salt = r();
    let witness = vec![];
    let contract = Contract::from(witness.as_slice());
    let contract = contract.address(&salt);

    let input = Input::coin(r(), r(), 0, r(), 0, maturity, predicate, predicate_data);
    let output = Output::contract_created(contract);

    let gas_price = 10;
    let gas_limit = 1_000_000;
    let bytecode_witness_index = 0;
    let static_contracts = vec![];
    let inputs = vec![input];
    let outputs = vec![output];
    let witnesses = vec![witness.into()];

    let mut tx = Transaction::create(
        gas_price,
        gas_limit,
        maturity,
        bytecode_witness_index,
        salt,
        static_contracts,
        inputs,
        outputs,
        witnesses,
    );

    let predicate_offset = tx.input_coin_predicate_offset(0).expect("Inconsistent inputs!");
    match tx.inputs_mut().get_mut(0) {
        Some(Input::Coin { predicate, .. }) => {
            let mut p = vec![];

            let predicate_data_offset =
                Interpreter::tx_mem_address() + predicate_offset + bytes::padded_len(predicate.as_slice()) + 16; // 4 additional opcodes represented as u32
            p.push(Opcode::ADDI(0x10, REG_ZERO, 0x08));
            p.push(Opcode::ADDI(0x11, REG_ZERO, predicate_data_offset as Immediate12));
            p.push(Opcode::MEQ(0x10, 0x11, 0x12, 0x10));
            p.push(Opcode::RET(0x10));

            let p: Vec<u8> = p.into_iter().map(|op| u32::from(op).to_be_bytes()).flatten().collect();

            predicate.extend_from_slice(p.as_slice());
        }
        _ => panic!("Inconsistent transaction!"),
    }

    vm.init(tx).expect("Failed to initialize VM");
    assert!(vm.run().is_err());
}
