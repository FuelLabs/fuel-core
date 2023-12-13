pub mod default_gas_costs;
pub mod import;

pub use fuel_core::database::Database;
pub use fuel_core_storage::vm_storage::VmStorage;
use fuel_core_types::{
    fuel_asm::{
        op,
        GTFArgs,
        Instruction,
        RegId,
    },
    fuel_tx::*,
    fuel_types::{
        BlockHeight,
        Word,
    },
    fuel_vm::{
        checked_transaction::{
            CheckPredicateParams,
            EstimatePredicates,
            IntoChecked,
        },
        consts::*,
        interpreter::{
            diff,
            InterpreterParams,
            ReceiptsCtx,
        },
        *,
    },
};

pub use rand::Rng;
use std::iter;

const LARGE_GAS_LIMIT: u64 = u64::MAX - 1001;

fn new_db() -> VmStorage<Database> {
    // when rocksdb is enabled, this creates a new db instance with a temporary path
    VmStorage::default()
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
    pub ra: RegId,
    pub rb: RegId,
    pub rc: RegId,
    pub rd: RegId,
}

pub struct VmBench {
    pub params: ConsensusParameters,
    pub gas_price: Word,
    pub gas_limit: Word,
    pub maturity: BlockHeight,
    pub height: BlockHeight,
    pub prepare_script: Vec<Instruction>,
    pub post_call: Vec<Instruction>,
    pub data: Vec<u8>,
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    pub witnesses: Vec<Witness>,
    pub db: Option<VmStorage<Database>>,
    pub instruction: Instruction,
    pub prepare_call: Option<PrepareCall>,
    pub dummy_contract: Option<ContractId>,
    pub contract_code: Option<ContractCode>,
    pub empty_contracts: Vec<ContractId>,
    pub receipts_ctx: Option<ReceiptsCtx>,
}

#[derive(Debug, Clone)]
pub struct VmBenchPrepared {
    pub vm: Interpreter<VmStorage<Database>, Script>,
    pub instruction: Instruction,
    pub diff: diff::Diff<diff::InitialVmState>,
}

impl VmBench {
    pub const SALT: Salt = Salt::zeroed();
    pub const CONTRACT: ContractId = ContractId::zeroed();

    pub fn new(instruction: Instruction) -> Self {
        let tx_params = TxParameters {
            max_gas_per_tx: LARGE_GAS_LIMIT + 1,
            ..Default::default()
        };
        Self {
            params: ConsensusParameters {
                tx_params,
                ..Default::default()
            },
            gas_price: 0,
            gas_limit: LARGE_GAS_LIMIT,
            maturity: Default::default(),
            height: Default::default(),
            prepare_script: vec![],
            post_call: vec![],
            data: vec![],
            inputs: vec![],
            outputs: vec![],
            witnesses: vec![],
            db: None,
            instruction,
            prepare_call: None,
            dummy_contract: None,
            contract_code: None,
            empty_contracts: vec![],
            receipts_ctx: None,
        }
    }

    pub fn contract<R>(rng: &mut R, instruction: Instruction) -> anyhow::Result<Self>
    where
        R: Rng,
    {
        Self::contract_using_db(rng, new_db(), instruction)
    }

    pub fn contract_using_db<R>(
        rng: &mut R,
        mut db: VmStorage<Database>,
        instruction: Instruction,
    ) -> anyhow::Result<Self>
    where
        R: Rng,
    {
        let bench = Self::new(instruction);

        let program = iter::once(instruction)
            .chain(iter::once(op::ret(RegId::ONE)))
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

        db.deploy_contract_with_id(&salt, &[], &contract, &state_root, &id)?;

        let data = id
            .iter()
            .copied()
            .chain((0 as Word).to_be_bytes().iter().copied())
            .chain((0 as Word).to_be_bytes().iter().copied())
            .chain(AssetId::default().iter().copied())
            .collect();

        let prepare_script = vec![
            op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
            op::addi(0x11, 0x10, ContractId::LEN as u16),
            op::addi(0x11, 0x11, WORD_SIZE as u16),
            op::addi(0x11, 0x11, WORD_SIZE as u16),
            op::movi(0x12, 100_000),
        ];

        let prepare_call = PrepareCall {
            ra: RegId::new(0x10),
            rb: RegId::ZERO,
            rc: RegId::new(0x11),
            rd: RegId::new(0x12),
        };

        Ok(bench
            .with_db(db)
            .with_data(data)
            .with_input(input)
            .with_output(output)
            .with_prepare_script(prepare_script)
            .with_prepare_call(prepare_call))
    }

    pub fn with_db(mut self, db: VmStorage<Database>) -> Self {
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

    pub fn with_script_gas_limit(mut self, gas_limit: Word) -> Self {
        self.gas_limit = gas_limit;
        self
    }

    pub fn with_maturity(mut self, maturity: BlockHeight) -> Self {
        self.maturity = maturity;
        self
    }

    pub fn with_height(mut self, height: BlockHeight) -> Self {
        self.height = height;
        self
    }

    /// Replaces the current prepare script with the given one.
    /// Not that if you've constructed this instance with `contract` or `using_contract_db`,
    /// then this will remove the script added by it. Use `extend_prepare_script` instead.
    pub fn with_prepare_script(mut self, prepare_script: Vec<Instruction>) -> Self {
        self.prepare_script = prepare_script;
        self
    }

    /// Adds more instructions before the current prepare script.
    pub fn prepend_prepare_script(mut self, prepare_script: Vec<Instruction>) -> Self {
        self.prepare_script.extend(prepare_script);
        self
    }

    pub fn with_post_call(mut self, post_call: Vec<Instruction>) -> Self {
        self.post_call = post_call;
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

    pub fn with_call_receipts(mut self, receipts_ctx: ReceiptsCtx) -> Self {
        self.receipts_ctx = Some(receipts_ctx);
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

    pub fn with_empty_contracts_count(mut self, count: usize) -> Self {
        let mut contract_ids = Vec::with_capacity(count);
        for n in 0..count {
            contract_ids.push(ContractId::from([n as u8; 32]));
        }
        self.empty_contracts = contract_ids;
        self
    }

    pub fn prepare(self) -> anyhow::Result<VmBenchPrepared> {
        self.try_into()
    }
}

impl TryFrom<VmBench> for VmBenchPrepared {
    type Error = anyhow::Error;

    fn try_from(case: VmBench) -> anyhow::Result<Self> {
        let VmBench {
            params,
            gas_price,
            gas_limit,
            maturity,
            height,
            prepare_script,
            post_call,
            data,
            inputs,
            outputs,
            witnesses,
            db,
            instruction,
            prepare_call,
            dummy_contract,
            contract_code,
            empty_contracts,
            receipts_ctx,
        } = case;

        let mut db = db.unwrap_or_else(new_db);

        if prepare_script
            .iter()
            .any(|op| matches!(op, Instruction::RET(_)))
        {
            return Err(anyhow::anyhow!(
                "a prepare script should not call/return into different contexts.",
            ))
        }

        let prepare_script = prepare_script
            .into_iter()
            .chain(iter::once(op::ret(RegId::ONE)))
            .chain(iter::once(instruction))
            .collect();

        let mut tx = TransactionBuilder::script(prepare_script, data);

        if let Some(contract) = dummy_contract {
            let code = iter::once(op::ret(RegId::ONE));
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
            let input_count = tx.inputs().len();
            let output =
                Output::contract(input_count as u8, Bytes32::zeroed(), Bytes32::zeroed());
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

        for contract_id in empty_contracts {
            let input_count = tx.inputs().len();
            let output =
                Output::contract(input_count as u8, Bytes32::zeroed(), Bytes32::zeroed());
            let input = Input::contract(
                UtxoId::default(),
                Bytes32::zeroed(),
                Bytes32::zeroed(),
                TxPointer::default(),
                contract_id,
            );

            tx.add_input(input);
            tx.add_output(output);

            db.deploy_contract_with_id(
                &VmBench::SALT,
                &[],
                &Contract::default(),
                &Bytes32::zeroed(),
                &contract_id,
            )?;
        }

        inputs.into_iter().for_each(|i| {
            tx.add_input(i);
        });

        outputs.into_iter().for_each(|o| {
            tx.add_output(o);
        });

        witnesses.into_iter().for_each(|w| {
            tx.add_witness(w);
        });

        // add at least one coin input
        tx.add_random_fee_input();

        let mut params = params;
        params.fee_params.gas_per_byte = 0;
        params.gas_costs = GasCosts::free();
        let mut tx = tx
            .gas_price(gas_price)
            .script_gas_limit(gas_limit)
            .maturity(maturity)
            .with_params(params.clone())
            .finalize();
        tx.estimate_predicates(&CheckPredicateParams::from(&params))
            .unwrap();
        let tx = tx.into_checked(height, &params).unwrap();

        let mut txtor = Transactor::new(db, InterpreterParams::from(&params));

        txtor.transact(tx);

        let mut vm: Interpreter<_, _> = txtor.into();

        if let Some(receipts_ctx) = receipts_ctx {
            *vm.receipts_mut() = receipts_ctx;
        }

        if let Some(p) = prepare_call {
            let PrepareCall { ra, rb, rc, rd } = p;

            vm.prepare_call(ra, rb, rc, rd)
                .map_err(anyhow::Error::msg)?;
            for instruction in post_call {
                vm.instruction(instruction).unwrap();
            }
        }

        let start_vm = vm.clone();
        let original_db = vm.as_mut().database_mut().clone();
        let database_tx = original_db.transaction().as_ref().clone();
        *vm.as_mut().database_mut() = database_tx;
        let mut vm = vm.add_recording();
        match instruction {
            Instruction::CALL(call) => {
                let (ra, rb, rc, rd) = call.unpack();
                vm.prepare_call(ra, rb, rc, rd).unwrap();
            }
            _ => {
                vm.instruction(instruction).unwrap();
            }
        }
        let storage_diff = vm.storage_diff();
        let mut vm = vm.remove_recording();
        let mut diff = start_vm.diff(&vm);
        diff += storage_diff;
        let diff: diff::Diff<diff::InitialVmState> = diff.into();
        vm.reset_vm_state(&diff);
        *vm.as_mut().database_mut() = original_db;

        Ok(Self {
            vm,
            instruction,
            diff,
        })
    }
}
