pub use fuel_core::database::Database;
use fuel_core_interfaces::common::fuel_tx::{
    StorageSlot,
    TransactionBuilder,
};
pub use fuel_core_interfaces::common::{
    consts::*,
    prelude::*,
};
pub use rand::Rng;
use std::{
    io,
    iter,
};

fn new_db() -> Database {
    // when rocksdb is enabled, this creates a new db instance with a temporary path
    Database::default()
}

pub struct ContractCode {
    pub contract: Contract,
    pub salt: Salt,
    pub id: ContractId,
    pub root: Bytes32,
    pub storage_root: Bytes32,
    pub slots: Vec<StorageSlot>,
}

impl From<Vec<u8>> for ContractCode {
    fn from(contract: Vec<u8>) -> Self {
        let contract = Contract::from(contract);
        let slots = vec![];
        let salt = VmBench::SALT;
        let storage_root = Contract::initial_state_root(slots.iter());
        let root = contract.root();
        let id = contract.id(&salt, &root, &storage_root);

        Self {
            contract,
            id,
            root,
            storage_root,
            salt,
            slots,
        }
    }
}

pub struct PrepareCall {
    ra: RegisterId,
    rb: RegisterId,
    rc: RegisterId,
    rd: RegisterId,
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
    pub prepare_call: Option<PrepareCall>,
    pub dummy_contract: Option<ContractId>,
    pub contract_code: Option<ContractCode>,
    pub prepare_db: Option<Box<dyn FnMut(Database) -> io::Result<Database>>>,
}

#[derive(Debug, Clone)]
pub struct VmBenchPrepared {
    pub vm: Interpreter<Database, Script>,
    pub instruction: Instruction,
}

impl VmBench {
    pub const SALT: Salt = Salt::zeroed();
    pub const CONTRACT: ContractId = ContractId::zeroed();

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
            prepare_call: None,
            dummy_contract: None,
            contract_code: None,
            prepare_db: None,
        }
    }

    pub fn contract<R>(rng: &mut R, instruction: Opcode) -> io::Result<Self>
    where
        R: Rng,
    {
        let bench = Self::new(instruction);

        let program = iter::once(instruction)
            .chain(iter::once(Opcode::RET(REG_ONE)))
            .collect::<Vec<u8>>();

        let program = Witness::from(program);

        let salt = rng.gen();

        let contract = Contract::from(program.as_ref());
        let state_root = Contract::default_state_root();
        let id = VmBench::CONTRACT;

        let utxo_id = rng.gen();
        let balance_root = rng.gen();
        let tx_pointer = rng.gen();

        let input = Input::contract(utxo_id, balance_root, state_root, tx_pointer, id);
        let output = Output::contract(0, rng.gen(), rng.gen());

        let mut db = new_db();

        db.deploy_contract_with_id(&salt, &[], &contract, &state_root, &id)?;

        let data = id
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

        let prepare_call = PrepareCall {
            ra: 0x10,
            rb: REG_ZERO,
            rc: 0x11,
            rd: 0x12,
        };

        Ok(bench
            .with_db(db)
            .with_data(data)
            .with_input(input)
            .with_output(output)
            .with_prepare_script(prepare_script)
            .with_prepare_call(prepare_call))
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

    pub fn with_prepare_call(mut self, call: PrepareCall) -> Self {
        self.prepare_call.replace(call);
        self
    }

    pub fn with_dummy_contract(mut self, dummy_contract: ContractId) -> Self {
        self.dummy_contract.replace(dummy_contract);
        self
    }

    pub fn with_contract_code(mut self, contract_code: ContractCode) -> Self {
        self.contract_code.replace(contract_code);
        self
    }

    pub fn with_prepare_db<F>(mut self, prepare_db: F) -> Self
    where
        F: FnMut(Database) -> io::Result<Database> + 'static,
    {
        self.prepare_db.replace(Box::new(prepare_db));
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

        let (op, ra, rb, rc, rd, _imm) = instruction.into_inner();

        match op {
            OpcodeRepr::CALL => Ok(vm
                .prepare_call(ra, rb, rc, rd)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?),
            _ => Ok(vm.instruction(instruction).map(|_| ())?),
        }
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
            prepare_call,
            dummy_contract,
            contract_code,
            prepare_db,
        } = case;

        let mut db = db.unwrap_or_else(new_db);

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

        if let Some(contract) = dummy_contract {
            let code = iter::once(Opcode::RET(REG_ONE));
            let code: Vec<u8> = code.collect();
            let code = Contract::from(code);
            let root = code.root();

            let input = tx.inputs().len();
            let output =
                Output::contract(input as u8, Bytes32::zeroed(), Bytes32::zeroed());
            let input = Input::contract(
                UtxoId::default(),
                Bytes32::zeroed(),
                Bytes32::zeroed(),
                TxPointer::default(),
                contract,
            );

            tx.add_input(input);
            tx.add_output(output);

            db.deploy_contract_with_id(&VmBench::SALT, &[], &code, &root, &contract)?;
        }

        if let Some(ContractCode {
            contract,
            salt,
            id,
            root,
            slots,
            storage_root,
        }) = contract_code
        {
            let input = tx.inputs().len();
            let output =
                Output::contract(input as u8, Bytes32::zeroed(), Bytes32::zeroed());
            let input = Input::contract(
                UtxoId::default(),
                Bytes32::zeroed(),
                storage_root,
                TxPointer::default(),
                id,
            );

            tx.add_input(input);
            tx.add_output(output);

            db.deploy_contract_with_id(&salt, &slots, &contract, &root, &id)?;
        }

        let db = match prepare_db {
            Some(mut prepare_db) => prepare_db(db)?,
            None => db,
        };

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

        let mut vm = txtor.interpreter();

        if let Some(p) = prepare_call {
            let PrepareCall { ra, rb, rc, rd } = p;

            vm.prepare_call(ra, rb, rc, rd)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        }

        Ok(Self { vm, instruction })
    }
}
