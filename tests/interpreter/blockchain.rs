use super::common::r;
use fuel_vm_rust::prelude::*;

#[test]
fn mint_burn() {
    let mut vm = Interpreter::default();

    let maturity = 100;
    let color: Color = r();
    let salt: Salt = r();
    let witness = vec![];
    let contract = Contract::from(witness.as_slice());
    let contract = contract.address(&salt);

    let input = Input::coin(r(), r(), 0, color, 0, maturity, vec![], vec![]);
    let output = Output::contract_created(contract);

    let gas_price = 10;
    let gas_limit = 1_000_000;
    let bytecode_witness_index = 0;
    let static_contracts = vec![];
    let inputs = vec![input];
    let outputs = vec![output];
    let witnesses = vec![witness.into()];

    let tx = Transaction::create(
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

    vm.init(tx).expect("Failed to initialize VM");
    vm.run().expect("Failed to execute mint");

    // TODO continue
}
