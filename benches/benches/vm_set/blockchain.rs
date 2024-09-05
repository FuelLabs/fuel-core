use std::sync::Arc;

use crate::utils::{
    linear,
    linear_short,
    make_receipts,
};

use super::run_group_ref;

use criterion::{
    Criterion,
    Throughput,
};
use fuel_core::{
    database::{
        balances::BalancesInitializer,
        database_description::on_chain::OnChain,
        state::StateInitializer,
        GenesisDatabase,
    },
    service::Config,
    state::historical_rocksdb::HistoricalRocksDB,
};
use fuel_core_benches::*;
use fuel_core_storage::{
    tables::FuelBlocks,
    transactional::{
        IntoTransaction,
        StorageTransaction,
    },
    vm_storage::{
        IncreaseStorageKey,
        VmStorage,
    },
    StorageAsMut,
};
use fuel_core_types::{
    blockchain::header::{
        ApplicationHeader,
        ConsensusHeader,
    },
    fuel_asm::{
        op,
        GTFArgs,
        RegId,
    },
    fuel_tx::{
        ContractIdExt,
        Input,
        Output,
        Word,
    },
    fuel_types::*,
    fuel_vm::consts::*,
    tai64::Tai64,
};
use rand::{
    rngs::StdRng,
    RngCore,
    SeedableRng,
};

pub struct BenchDb {
    db: GenesisDatabase,
    /// Used for RAII cleanup. Contents of this directory are deleted on drop.
    _tmp_dir: utils::ShallowTempDir,
}

impl BenchDb {
    fn new(contract_id: &ContractId) -> anyhow::Result<Self> {
        let tmp_dir = utils::ShallowTempDir::new();

        let db = HistoricalRocksDB::<OnChain>::default_open(
            tmp_dir.path(),
            None,
            Default::default(),
        )
        .unwrap();
        let db = Arc::new(db);
        let mut storage_key = primitive_types::U256::zero();
        let mut key_bytes = Bytes32::zeroed();

        let state_size = crate::utils::get_state_size();

        let mut database = GenesisDatabase::new(db);
        database.init_contract_state(
            contract_id,
            (0..state_size).map(|_| {
                storage_key.to_big_endian(key_bytes.as_mut());
                storage_key.increase().unwrap();
                (key_bytes, key_bytes.to_vec())
            }),
        )?;

        let mut storage_key = primitive_types::U256::zero();
        let mut sub_id = Bytes32::zeroed();
        database.init_contract_balances(
            contract_id,
            (0..state_size).map(|k| {
                storage_key.to_big_endian(sub_id.as_mut());

                let asset = if k % 2 == 0 {
                    VmBench::CONTRACT.asset_id(&sub_id)
                } else {
                    let asset_id = AssetId::new(*sub_id);
                    storage_key.increase().unwrap();
                    asset_id
                };
                (asset, k / 2 + 1_000)
            }),
        )?;
        // Adds a genesis block to the database.
        let config = Config::local_node();
        let block = fuel_core::service::genesis::create_genesis_block(&config);
        let chain_config = config.snapshot_reader.chain_config();
        database
            .storage::<FuelBlocks>()
            .insert(
                &0u32.into(),
                &block.compress(&chain_config.consensus_parameters.chain_id()),
            )
            .unwrap();

        Ok(Self {
            _tmp_dir: tmp_dir,
            db: database,
        })
    }

    /// Creates a `VmDatabase` instance.
    fn to_vm_database(&self) -> VmStorage<StorageTransaction<GenesisDatabase>> {
        let consensus = ConsensusHeader {
            prev_root: Default::default(),
            height: 1.into(),
            time: Tai64::UNIX_EPOCH,
            generated: (),
        };
        let application = ApplicationHeader {
            da_height: Default::default(),
            consensus_parameters_version: 0,
            generated: (),
            state_transition_bytecode_version: 0,
        };
        VmStorage::new(
            self.db.clone().into_transaction(),
            &consensus,
            &application,
            ContractId::zeroed(),
        )
    }
}

pub fn run(c: &mut Criterion) {
    let rng = &mut StdRng::seed_from_u64(2322u64);

    let linear_short = linear_short();
    let linear = linear();

    let asset = AssetId::zeroed();
    let contract: ContractId = VmBench::CONTRACT;

    let db = BenchDb::new(&contract).expect("Unable to fill contract storage");

    let receipts_ctx = make_receipts(rng);

    run_group_ref(
        &mut c.benchmark_group("bal"),
        "bal",
        VmBench::new(op::bal(0x13, 0x10, 0x11))
            .with_db(db.to_vm_database())
            .with_data(asset.iter().chain(contract.iter()).copied().collect())
            .with_prepare_script(vec![
                op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
                op::addi(0x11, 0x10, asset.len().try_into().unwrap()),
            ])
            .with_dummy_contract(contract),
    );

    {
        let mut start_key = Bytes32::zeroed();
        // The checkpoint was initialized with entries starting `0..STATE_SIZE`.
        // We want to write new entry to the database, so the starting key is far.
        start_key.as_mut()[0] = 255;
        let data = start_key.iter().copied().collect::<Vec<_>>();

        let post_call = vec![
            op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
            op::addi(0x11, 0x10, ContractId::LEN.try_into().unwrap()),
            op::addi(0x11, 0x11, WORD_SIZE.try_into().unwrap()),
            op::addi(0x11, 0x11, WORD_SIZE.try_into().unwrap()),
            op::addi(0x11, 0x11, AssetId::LEN.try_into().unwrap()),
        ];
        let mut bench = VmBench::contract_using_db(
            rng,
            db.to_vm_database(),
            op::sww(0x11, 0x29, RegId::ONE),
        )
        .expect("failed to prepare contract")
        .with_post_call(post_call);
        bench.data.extend(data);

        run_group_ref(&mut c.benchmark_group("sww"), "sww", bench);
    }

    {
        let input = VmBench::contract_using_db(
            rng,
            db.to_vm_database(),
            op::srw(0x13, 0x14, 0x10),
        )
        .expect("failed to prepare contract");
        run_group_ref(&mut c.benchmark_group("srw"), "srw", input);
    }

    let mut scwq = c.benchmark_group("scwq");

    for i in linear_short.clone() {
        // We want to clear entries from the checkpoint, so the starting key is zero.
        let start_key = Bytes32::zeroed();
        let data = start_key.iter().copied().collect::<Vec<_>>();

        let post_call = vec![
            op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
            op::addi(0x11, 0x10, ContractId::LEN.try_into().unwrap()),
            op::addi(0x11, 0x11, WORD_SIZE.try_into().unwrap()),
            op::addi(0x11, 0x11, WORD_SIZE.try_into().unwrap()),
            op::addi(0x11, 0x11, AssetId::LEN.try_into().unwrap()),
            op::movi(0x12, i as u32),
        ];
        let mut bench = VmBench::contract_using_db(
            rng,
            db.to_vm_database(),
            op::scwq(0x11, 0x29, 0x12),
        )
        .expect("failed to prepare contract")
        .with_post_call(post_call);
        bench.data.extend(data);

        scwq.throughput(Throughput::Bytes(i));

        run_group_ref(&mut scwq, format!("{i}"), bench);
    }

    scwq.finish();

    let mut swwq = c.benchmark_group("swwq");

    for i in linear_short.clone() {
        let mut start_key = Bytes32::zeroed();
        // The checkpoint was initialized with entries starting `0..STATE_SIZE`.
        // We want to write new entries to the database, so the starting key is far.
        start_key.as_mut()[0] = 255;
        let data = start_key.iter().copied().collect::<Vec<_>>();

        let post_call = vec![
            op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
            op::addi(0x11, 0x10, ContractId::LEN.try_into().unwrap()),
            op::addi(0x11, 0x11, WORD_SIZE.try_into().unwrap()),
            op::addi(0x11, 0x11, WORD_SIZE.try_into().unwrap()),
            op::addi(0x11, 0x11, AssetId::LEN.try_into().unwrap()),
            op::movi(0x12, i as u32),
            // Allocate space for storage values in the memory
            op::cfei(i as u32 * Bytes32::LEN as u32),
        ];
        let mut bench = VmBench::contract_using_db(
            rng,
            db.to_vm_database(),
            op::swwq(0x11, 0x20, RegId::ZERO, 0x12),
        )
        .expect("failed to prepare contract")
        .with_post_call(post_call);
        bench.data.extend(data);

        swwq.throughput(Throughput::Bytes(i));

        run_group_ref(&mut swwq, format!("{i}"), bench);
    }

    swwq.finish();

    let mut call = c.benchmark_group("call");

    for i in linear.clone() {
        let mut code = vec![123u8; i as usize];

        rng.fill_bytes(&mut code);

        let mut code = ContractCode::from(code);
        code.id = VmBench::CONTRACT;

        let data = code
            .id
            .iter()
            .copied()
            .chain((0 as Word).to_be_bytes().iter().copied())
            .chain((0 as Word).to_be_bytes().iter().copied())
            .chain(AssetId::default().iter().copied())
            .collect();

        let prepare_script = vec![
            op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
            op::addi(0x11, 0x10, ContractId::LEN.try_into().unwrap()),
            op::addi(0x11, 0x11, WORD_SIZE.try_into().unwrap()),
            op::addi(0x11, 0x11, WORD_SIZE.try_into().unwrap()),
            op::movi(0x12, 100_000),
        ];

        call.throughput(Throughput::Bytes(i));

        run_group_ref(
            &mut call,
            format!("{i}"),
            VmBench::new(op::call(0x10, RegId::ZERO, 0x11, RegId::CGAS))
                .with_db(db.to_vm_database())
                .with_contract_code(code)
                .with_data(data)
                .with_prepare_script(prepare_script)
                .with_call_receipts(receipts_ctx.clone()),
        );
    }

    call.finish();

    let mut ldc = c.benchmark_group("ldc");

    for i in linear.clone() {
        let mut code = vec![0u8; i as usize];

        rng.fill_bytes(&mut code);

        let mut code = ContractCode::from(code);
        code.id = VmBench::CONTRACT;

        let data = code
            .id
            .iter()
            .copied()
            .chain((0 as Word).to_be_bytes().iter().copied())
            .chain((0 as Word).to_be_bytes().iter().copied())
            .chain(AssetId::default().iter().copied())
            .collect();

        let prepare_script = vec![
            op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
            op::addi(0x11, 0x10, ContractId::LEN.try_into().unwrap()),
            op::addi(0x11, 0x11, WORD_SIZE.try_into().unwrap()),
            op::addi(0x11, 0x11, WORD_SIZE.try_into().unwrap()),
            op::movi(0x12, 100_000),
            op::movi(0x13, i.try_into().unwrap()),
        ];

        ldc.throughput(Throughput::Bytes(i));

        run_group_ref(
            &mut ldc,
            format!("{i}"),
            VmBench::new(op::ldc(0x10, RegId::ZERO, 0x13, 0))
                .with_contract_code(code)
                .with_data(data)
                .with_prepare_script(prepare_script),
        );
    }

    ldc.finish();

    let mut ccp = c.benchmark_group("ccp");

    for i in linear.clone() {
        let mut code = vec![op::noop(); i as usize].into_iter().collect::<Vec<_>>();

        rng.fill_bytes(&mut code);

        let mut code = ContractCode::from(code);
        code.id = VmBench::CONTRACT;

        let data = code
            .id
            .iter()
            .copied()
            .chain((0 as Word).to_be_bytes().iter().copied())
            .chain((0 as Word).to_be_bytes().iter().copied())
            .chain(AssetId::default().iter().copied())
            .collect();

        let prepare_script = vec![
            op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
            op::addi(0x11, 0x10, ContractId::LEN.try_into().unwrap()),
            op::addi(0x11, 0x11, WORD_SIZE.try_into().unwrap()),
            op::addi(0x11, 0x11, WORD_SIZE.try_into().unwrap()),
            op::movi(0x12, 100_000),
            op::movi(0x13, i.try_into().unwrap()),
            op::cfe(0x13),
            op::movi(0x14, i.try_into().unwrap()),
            op::movi(0x15, i.try_into().unwrap()),
            op::add(0x15, 0x15, 0x15),
            op::addi(0x15, 0x15, 32),
            op::aloc(0x15),
            op::move_(0x15, RegId::HP),
        ];

        ccp.throughput(Throughput::Bytes(i));

        run_group_ref(
            &mut ccp,
            format!("{i}"),
            VmBench::new(op::ccp(0x15, 0x10, RegId::ZERO, 0x13))
                .with_contract_code(code)
                .with_data(data)
                .with_prepare_script(prepare_script),
        );
    }

    ccp.finish();

    let mut csiz = c.benchmark_group("csiz");

    for i in linear.clone() {
        let mut code = vec![0u8; i as usize];

        rng.fill_bytes(&mut code);

        let mut code = ContractCode::from(code);
        code.id = VmBench::CONTRACT;

        let data = code.id.iter().copied().collect();

        let prepare_script = vec![op::gtf_args(0x10, 0x00, GTFArgs::ScriptData)];

        csiz.throughput(Throughput::Bytes(i));

        run_group_ref(
            &mut csiz,
            format!("{i}"),
            VmBench::new(op::csiz(0x11, 0x10))
                .with_contract_code(code)
                .with_data(data)
                .with_prepare_script(prepare_script),
        );
    }

    csiz.finish();

    run_group_ref(
        &mut c.benchmark_group("bhei"),
        "bhei",
        VmBench::new(op::bhei(0x10)),
    );

    run_group_ref(
        &mut c.benchmark_group("bhsh"),
        "bhsh",
        VmBench::new(op::bhsh(0x10, RegId::ZERO)).with_prepare_script(vec![
            op::movi(0x10, Bytes32::LEN.try_into().unwrap()),
            op::aloc(0x10),
            op::move_(0x10, RegId::HP),
        ]),
    );

    run_group_ref(
        &mut c.benchmark_group("mint"),
        "mint",
        VmBench::contract_using_db(
            rng,
            db.to_vm_database(),
            op::mint(RegId::ONE, RegId::ZERO),
        )
        .expect("failed to prepare contract")
        .with_call_receipts(receipts_ctx.clone()),
    );

    run_group_ref(
        &mut c.benchmark_group("burn"),
        "burn",
        VmBench::contract_using_db(
            rng,
            db.to_vm_database(),
            op::burn(RegId::ONE, RegId::HP),
        )
        .expect("failed to prepare contract")
        .prepend_prepare_script(vec![op::movi(0x10, 32), op::aloc(0x10)])
        .with_call_receipts(receipts_ctx.clone()),
    );

    run_group_ref(
        &mut c.benchmark_group("cb"),
        "cb",
        VmBench::new(op::cb(0x10)).with_prepare_script(vec![
            op::movi(0x10, Bytes32::LEN.try_into().unwrap()),
            op::aloc(0x10),
            op::move_(0x10, RegId::HP),
        ]),
    );

    // tr
    {
        let mut input = VmBench::contract_using_db(
            rng,
            db.to_vm_database(),
            op::tr(0x15, 0x14, 0x15),
        )
        .expect("failed to prepare contract")
        .with_call_receipts(receipts_ctx.clone());
        input
            .prepare_script
            .extend(vec![op::movi(0x15, 2000), op::movi(0x14, 100)]);
        run_group_ref(&mut c.benchmark_group("tr"), "tr", input);
    }

    // tro
    {
        let mut input = VmBench::contract_using_db(
            rng,
            db.to_vm_database(),
            op::tro(RegId::ZERO, 0x15, 0x14, RegId::HP),
        )
        .expect("failed to prepare contract")
        .with_call_receipts(receipts_ctx.clone());
        let coin_output = Output::variable(Address::zeroed(), 100, AssetId::zeroed());
        input.outputs.push(coin_output);
        let predicate = op::ret(RegId::ONE).to_bytes().to_vec();
        let owner = Input::predicate_owner(&predicate);
        let coin_input = Input::coin_predicate(
            Default::default(),
            owner,
            1000,
            AssetId::zeroed(),
            Default::default(),
            Default::default(),
            predicate,
            vec![],
        );
        input.inputs.push(coin_input);

        let index = input.outputs.len() - 1;
        input.prepare_script.extend(vec![
            op::movi(0x14, 100),
            op::movi(0x15, index.try_into().unwrap()),
            op::movi(0x20, 32),
            op::aloc(0x20),
        ]);
        for (i, v) in (*AssetId::zeroed()).into_iter().enumerate() {
            input.prepare_script.push(op::movi(0x20, v as u32));
            input.prepare_script.push(op::sb(RegId::HP, 0x20, i as u16));
        }

        run_group_ref(&mut c.benchmark_group("tro"), "tro", input);
    }

    run_group_ref(
        &mut c.benchmark_group("cfsi"),
        "cfsi",
        VmBench::new(op::cfsi(0)),
    );

    let mut croo = c.benchmark_group("croo");

    for i in linear.clone() {
        let mut code = vec![0u8; i as usize];

        rng.fill_bytes(&mut code);

        let mut code = ContractCode::from(code);
        code.id = VmBench::CONTRACT;

        let data = code.id.iter().copied().collect();

        let prepare_script = vec![
            op::gtf_args(0x16, 0x00, GTFArgs::ScriptData),
            op::movi(0x15, 2000),
            op::aloc(0x15),
            op::move_(0x14, RegId::HP),
        ];

        croo.throughput(Throughput::Bytes(i));

        run_group_ref(
            &mut croo,
            format!("{i}"),
            VmBench::new(op::croo(0x14, 0x16))
                .with_contract_code(code)
                .with_data(data)
                .with_prepare_script(prepare_script),
        );
    }

    croo.finish();

    run_group_ref(
        &mut c.benchmark_group("flag"),
        "flag",
        VmBench::new(op::flag(0x10)),
    );

    run_group_ref(
        &mut c.benchmark_group("gm"),
        "gm",
        VmBench::contract(rng, op::gm(0x10, 1)).unwrap(),
    );

    // smo
    {
        let mut smo = c.benchmark_group("smo");

        for i in linear.clone() {
            let mut input = VmBench::contract_using_db(
                rng,
                db.to_vm_database(),
                op::smo(0x15, 0x16, 0x17, 0x18),
            )
            .expect("failed to prepare contract")
            .with_call_receipts(receipts_ctx.clone());
            input.post_call.extend(vec![
                op::gtf_args(0x15, 0x00, GTFArgs::ScriptData),
                // Offset 32 + 8 + 8 + 32
                op::addi(0x15, 0x15, 32 + 8 + 8 + 32), // target address pointer
                op::addi(0x16, 0x15, 32),              // data ppinter
                op::movi(0x17, i.try_into().unwrap()), // data length
                op::movi(0x18, 10),                    // coins to send
            ]);
            input.data.extend(
                Address::new([1u8; 32])
                    .iter()
                    .copied()
                    .chain(vec![2u8; i as usize]),
            );
            let predicate = op::ret(RegId::ONE).to_bytes().to_vec();
            let owner = Input::predicate_owner(&predicate);
            let coin_input = Input::coin_predicate(
                Default::default(),
                owner,
                Word::MAX >> 2,
                AssetId::zeroed(),
                Default::default(),
                Default::default(),
                predicate,
                vec![],
            );
            input.inputs.push(coin_input);
            smo.throughput(Throughput::Bytes(i));
            run_group_ref(&mut smo, format!("{i}"), input);
        }

        smo.finish();
    }

    {
        let mut srwq = c.benchmark_group("srwq");

        for i in linear_short.clone() {
            // We want to iterate over initialized entries starting key 0.
            let start_key = Bytes32::zeroed();
            let data = start_key.iter().copied().collect::<Vec<_>>();

            let post_call = vec![
                op::movi(0x16, i as u32),
                op::movi(0x17, 2000),
                op::move_(0x15, 0x16),
                op::muli(0x15, 0x15, 32),
                op::addi(0x15, 0x15, 1),
                op::aloc(0x15),
                op::move_(0x14, RegId::HP),
                op::gtf_args(0x27, 0x00, GTFArgs::ScriptData),
                op::addi(0x27, 0x27, ContractId::LEN.try_into().unwrap()),
                op::addi(0x27, 0x27, WORD_SIZE.try_into().unwrap()),
                op::addi(0x27, 0x27, WORD_SIZE.try_into().unwrap()),
                op::addi(0x27, 0x27, AssetId::LEN.try_into().unwrap()),
            ];
            let mut bench = VmBench::contract_using_db(
                rng,
                db.to_vm_database(),
                op::srwq(0x14, 0x11, 0x27, 0x16),
            )
            .expect("failed to prepare contract")
            .with_post_call(post_call);
            bench.data.extend(data);
            srwq.throughput(Throughput::Bytes(i));
            run_group_ref(&mut srwq, format!("{i}"), bench);
        }

        srwq.finish();
    }

    // time
    {
        run_group_ref(
            &mut c.benchmark_group("time"),
            "time",
            VmBench::new(op::time(0x11, 0x10))
                .with_db(db.to_vm_database())
                .with_prepare_script(vec![op::movi(0x10, 0)]),
        );
    }
}
