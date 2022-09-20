use std::{
    io,
    iter,
};

use fuel_core_interfaces::common::fuel_tx::TransactionBuilder;
use tempfile::TempDir;

pub use fuel_core::database::Database;
pub use fuel_core_interfaces::common::{
    consts::*,
    prelude::*,
};
pub use rand::Rng;

fn new_db() -> io::Result<Database> {
    TempDir::new().and_then(|t| Database::open(t.as_ref()).map_err(|e| e.into()))
}

pub struct VmBench {
    pub params: ConsensusParameters,
    pub gas_price: Word,
    pub gas_limit: Word,
    pub maturity: Word,
    pub height: Word,
    pub prepare_script: Vec<Opcode>,
    pub data: Vec<u8>,
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    pub witnesses: Vec<Witness>,
    pub db: Option<Database>,
    pub instruction: Opcode,
}

#[derive(Debug, Clone)]
pub struct VmBenchPrepared {
    pub vm: Interpreter<Database>,
    pub instruction: Instruction,
}

impl VmBench {
    pub fn new(instruction: Opcode) -> Self {
        Self {
            params: ConsensusParameters::default(),
            gas_price: 0,
            gas_limit: 1_000_000,
            maturity: 0,
            height: 0,
            prepare_script: vec![],
            data: vec![],
            inputs: vec![],
            outputs: vec![],
            witnesses: vec![],
            db: None,
            instruction,
        }
    }

    pub fn contract<R>(rng: &mut R, instruction: Opcode) -> io::Result<Self>
    where
        R: Rng,
    {
        let bench = Self::new(Opcode::CALL(0x10, REG_ZERO, 0x11, 0x12));

        let program = iter::once(instruction)
            .chain(iter::once(Opcode::RET(REG_ONE)))
            .collect::<Vec<u8>>();

        let program = Witness::from(program);

        let salt = rng.gen();

        let contract = Contract::from(program.as_ref());
        let contract_root = contract.root();
        let state_root = Contract::default_state_root();
        let contract = contract.id(&salt, &contract_root, &state_root);

        let utxo_id = rng.gen();
        let balance_root = rng.gen();
        let tx_pointer = rng.gen();

        let input =
            Input::contract(utxo_id, balance_root, state_root, tx_pointer, contract);
        let output = Output::contract_created(contract, state_root);

        let bytecode_witness = 0;
        let tx = Transaction::create(
            bench.gas_price,
            bench.gas_limit,
            bench.maturity,
            bytecode_witness,
            salt,
            vec![],
            vec![],
            vec![output],
            vec![program],
        )
        .check(bench.height, &bench.params)?;

        let output = Output::contract(0, rng.gen(), rng.gen());

        let mut db = new_db()?;

        let mut txtor = Transactor::new(&mut db, bench.params);

        txtor.transact(tx);

        let data = contract
            .iter()
            .copied()
            .chain((0 as Word).to_be_bytes().iter().copied())
            .chain((0 as Word).to_be_bytes().iter().copied())
            .chain(AssetId::default().iter().copied())
            .collect();

        let prepare_script = vec![
            Opcode::gtf(0x10, 0x00, GTFArgs::ScriptData),
            Opcode::ADDI(0x11, 0x10, ContractId::LEN as Immediate12),
            Opcode::ADDI(0x11, 0x11, WORD_SIZE as Immediate12),
            Opcode::ADDI(0x11, 0x11, WORD_SIZE as Immediate12),
            Opcode::MOVI(0x12, 100_000),
        ];

        Ok(bench
            .with_db(db)
            .with_data(data)
            .with_input(input)
            .with_output(output)
            .with_prepare_script(prepare_script))
    }

    pub fn with_db(mut self, db: Database) -> Self {
        self.db.replace(db);
        self
    }

    pub fn with_params(mut self, params: ConsensusParameters) -> Self {
        self.params = params;
        self
    }

    pub fn with_gas_price(mut self, gas_price: Word) -> Self {
        self.gas_price = gas_price;
        self
    }

    pub fn with_gas_limit(mut self, gas_limit: Word) -> Self {
        self.gas_limit = gas_limit;
        self
    }

    pub fn with_maturity(mut self, maturity: Word) -> Self {
        self.maturity = maturity;
        self
    }

    pub fn with_height(mut self, height: Word) -> Self {
        self.height = height;
        self
    }

    pub fn with_prepare_script(mut self, prepare_script: Vec<Opcode>) -> Self {
        self.prepare_script = prepare_script;
        self
    }

    pub fn with_data(mut self, data: Vec<u8>) -> Self {
        self.data = data;
        self
    }

    pub fn with_input(mut self, input: Input) -> Self {
        self.inputs.push(input);
        self
    }

    pub fn with_output(mut self, output: Output) -> Self {
        self.outputs.push(output);
        self
    }

    pub fn with_witness(mut self, witness: Witness) -> Self {
        self.witnesses.push(witness);
        self
    }

    pub fn prepare(self) -> io::Result<VmBenchPrepared> {
        self.try_into()
    }
}

impl VmBenchPrepared {
    pub fn run(self) -> io::Result<()> {
        let Self {
            mut vm,
            instruction,
        } = self;

        Ok(vm.instruction(instruction).map(|_| ())?)
    }
}

impl TryFrom<VmBench> for VmBenchPrepared {
    type Error = io::Error;

    fn try_from(case: VmBench) -> io::Result<Self> {
        let VmBench {
            params,
            gas_price,
            gas_limit,
            maturity,
            height,
            prepare_script,
            data,
            inputs,
            outputs,
            witnesses,
            db,
            instruction,
        } = case;

        let db = db.map(Ok).unwrap_or_else(new_db)?;

        if prepare_script.iter().any(|op| matches!(op, Opcode::RET(_))) {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "a prepare script should not call/return into different contexts.",
            ))
        }

        let prepare_script = prepare_script
            .into_iter()
            .chain(iter::once(Opcode::RET(REG_ONE)))
            .chain(iter::once(instruction))
            .collect();

        let mut tx = TransactionBuilder::script(prepare_script, data);

        inputs.into_iter().for_each(|i| {
            tx.add_input(i);
        });

        outputs.into_iter().for_each(|o| {
            tx.add_output(o);
        });

        witnesses.into_iter().for_each(|w| {
            tx.add_witness(w);
        });

        let tx = tx
            .gas_price(gas_price)
            .gas_limit(gas_limit)
            .maturity(maturity)
            .finalize_checked(height, &params);

        let instruction = Instruction::from(instruction);

        let mut txtor = Transactor::new(db, params);

        txtor.transact(tx);

        Ok(Self {
            vm: txtor.interpreter(),
            instruction,
        })
    }
}
