use fuel_core::consts::*;
use fuel_core::prelude::*;

mod alu;
mod blockchain;
mod crypto;
mod flow;
mod frame;
mod memory;
mod predicate;

pub use super::common;

pub fn program_to_bytes(program: &[Opcode]) -> Vec<u8> {
    program
        .iter()
        .map(|op| u32::from(*op).to_be_bytes())
        .flatten()
        .collect()
}

pub fn deploy_contract<S>(
    gas_price: Word,
    gas_limit: Word,
    maturity: Word,
    vm: &mut Interpreter<S>,
    program: &[Opcode],
) -> ContractAddress
where
    S: Storage<ContractAddress, Contract> + Storage<Color, Word>,
{
    let salt: Salt = common::r();
    let program = Witness::from(program_to_bytes(program));

    let contract = Contract::from(program.as_ref()).address(salt.as_ref());
    let output = Output::contract_created(contract);

    // Deploy the contract
    let tx = Transaction::create(
        gas_price,
        gas_limit,
        maturity,
        0,
        salt,
        vec![],
        vec![],
        vec![output],
        vec![program.clone()],
    );

    vm.init(tx).expect("Failed to init VM with tx create!");
    vm.run().expect("Failed to deploy contract!");

    contract
}
