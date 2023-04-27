use std::collections::HashSet;

use criterion::{
    criterion_group,
    criterion_main,
    measurement::WallTime,
    BenchmarkGroup,
    BenchmarkId,
    Criterion,
};
use enum_iterator::{
    all,
    Sequence,
};
use fuel_core::{
    executor::Executor,
    service::{
        adapters::MaybeRelayerAdapter,
        Config,
    },
};
use fuel_core_benches::Database;
use fuel_core_storage::{
    tables::{
        Coins,
        ContractsLatestUtxo,
    },
    StorageAsMut,
};
use fuel_core_types::{
    blockchain::{
        block::PartialFuelBlock,
        header::{
            ApplicationHeader,
            ConsensusHeader,
            PartialBlockHeader,
        },
    },
    entities::{
        coins::coin::CompressedCoin,
        contract::ContractUtxoInfo,
    },
    fuel_asm::op,
    fuel_crypto::generate_mnemonic_phrase,
    fuel_tx::{
        Cacheable,
        ConsensusParameters,
        Executable,
        Finalizable,
        Input,
        Output,
        Signable,
        Transaction,
        TransactionBuilder,
        TxPointer,
        UtxoId,
        Witness,
    },
    fuel_types::{
        Address,
        AssetId,
        BlockHeight,
        Bytes32,
        ContractId,
        Nonce,
        Word,
    },
    fuel_vm::{
        GasCosts,
        Salt,
        SecretKey,
    },
    services::executor::ExecutionBlock,
};
use itertools::Itertools;
use rand::{
    rngs::StdRng,
    SeedableRng,
};

#[derive(Default, Debug, Clone)]
struct SignedCoin {
    secret: SecretKey,
    utxo_id: UtxoId,
    amount: Word,
    asset_id: AssetId,
    tx_pointer: TxPointer,
    maturity: BlockHeight,
}

#[derive(Default, Debug, Clone)]
struct SignedMessage {
    secret: SecretKey,
    sender: Address,
    recipient: Address,
    nonce: Nonce,
    amount: Word,
    data: Vec<u8>,
}

#[derive(Default)]
struct Inputs {
    scripts: Scripts,
    creates: Creates,
    measure: usize,
}

enum InputToOutput {
    CoinToVoid(usize),
    CoinToCoin(usize),
    CoinToChange(HashSet<[u8; 32]>),
    CoinToVariable(usize),
    CoinToContractCreated(usize),
    MessageToVoid(usize),
    MessageToCoin(usize),
    MessageToChange(HashSet<[u8; 32]>),
    MessageToVariable(usize),
    MessageToContractCreated(usize),
    ContractToContract(usize),
    VoidToMessage(usize),
    VoidToCoin(usize),
    VoidToChange(HashSet<[u8; 32]>),
    VoidToVariable(usize),
    VoidContractCreated(usize),
}

#[derive(Default, Clone, Debug)]
struct Scripts {
    coins_per_script: usize,
    messages_per_script: usize,
    script_size: usize,
    script_data_size: usize,
    message_data_size: usize,
    number_extra_witnesses: usize,
    extra_witnesses_data: usize,
    num_outputs: usize,
}

#[derive(Default, Clone, Debug)]
struct Creates {
    num_outputs: usize,
}

fn txn(c: &mut Criterion) {
    let mut database = Database::default();
    let relayer = MaybeRelayerAdapter {
        database: database.clone(),
        relayer_synced: None,
        da_deploy_height: 0u64.into(),
    };
    let mut config = Config::local_node();
    config.chain_conf.gas_costs = GasCosts::free();

    // for (contract_id, id) in array_32s::<C>().map(ContractId::from).zip(utxo_ids::<C>(1))
    // {
    //     database
    //         .storage::<ContractsLatestUtxo>()
    //         .insert(
    //             &contract_id,
    //             &ContractUtxoInfo {
    //                 utxo_id: id,
    //                 tx_pointer: Default::default(),
    //             },
    //         )
    //         .unwrap();
    // }

    config.utxo_validation = true;
    let mut executor = Executor {
        database,
        relayer,
        config,
    };

    let mut execute = c.benchmark_group("execute_without_commit");

    let select = std::env::var_os("SELECT_BENCH")
        .map(|s| s.to_string_lossy().parse::<usize>().unwrap());

    if select.is_none() || matches!(select, Some(0)) {
        let mut i = 0;
        for num_inputs in [1, 100, 200] {
            measure::<20, 2, 0>(
                Inputs {
                    scripts: Scripts {
                        coins_per_script: num_inputs,
                        script_size: 100,
                        script_data_size: 100,
                        ..Default::default()
                    },
                    measure: num_inputs,
                    ..Default::default()
                },
                &mut i,
                "inputs",
                &mut executor,
                &mut execute,
            );
        }
    }
    if select.is_none() || matches!(select, Some(1)) {
        let mut i = 0;
        for script_size in [1000, 10_000, 100_000] {
            measure::<2, 2, 0>(
                Inputs {
                    scripts: Scripts {
                        coins_per_script: 1,
                        script_size,
                        script_data_size: 100,
                        ..Default::default()
                    },
                    measure: script_size,
                    ..Default::default()
                },
                &mut i,
                "script_size",
                &mut executor,
                &mut execute,
            );
        }
    }
    if select.is_none() || matches!(select, Some(2)) {
        let mut i = 0;
        for script_data_size in [1000, 10_000, 100_000] {
            measure::<2, 2, 0>(
                Inputs {
                    scripts: Scripts {
                        coins_per_script: 1,
                        script_size: 100,
                        script_data_size,
                        ..Default::default()
                    },
                    measure: script_data_size,
                    ..Default::default()
                },
                &mut i,
                "script_data_size",
                &mut executor,
                &mut execute,
            );
        }
    }
    if select.is_none() || matches!(select, Some(3)) {
        let mut i = 0;
        for extra_witnesses_data in [1000, 10_000, 100_000] {
            measure::<2, 2, 0>(
                Inputs {
                    scripts: Scripts {
                        coins_per_script: 1,
                        script_size: 100,
                        script_data_size: 100,
                        extra_witnesses_data,
                        number_extra_witnesses: 1,
                        ..Default::default()
                    },
                    measure: extra_witnesses_data,
                    ..Default::default()
                },
                &mut i,
                "extra_witnesses_data",
                &mut executor,
                &mut execute,
            );
        }
    }
    if select.is_none() || matches!(select, Some(4)) {
        let mut i = 0;
        for number_extra_witnesses in [10, 100] {
            measure::<2, 2, 0>(
                Inputs {
                    scripts: Scripts {
                        coins_per_script: 1,
                        script_size: 100,
                        script_data_size: 100,
                        extra_witnesses_data: 32,
                        number_extra_witnesses,
                        ..Default::default()
                    },
                    measure: number_extra_witnesses,
                    ..Default::default()
                },
                &mut i,
                "number_extra_witnesses",
                &mut executor,
                &mut execute,
            );
        }
    }
    if select.is_none() || matches!(select, Some(5)) {
        let mut i = 0;
        for (coins_per_script, messages_per_script, num_outputs) in [(1, 0, 1)] {
            measure::<20, 20, 0>(
                Inputs {
                    scripts: Scripts {
                        coins_per_script,
                        script_size: 100,
                        script_data_size: 100,
                        extra_witnesses_data: 32,
                        number_extra_witnesses: 100,
                        messages_per_script,
                        message_data_size: 100,
                        num_outputs,
                    },
                    measure: num_outputs,
                    ..Default::default()
                },
                &mut i,
                "number_of_outputs",
                &mut executor,
                &mut execute,
            );
        }
    }
}

fn measure<const DB_COINS: u64, const SCRIPTS: u64, const CREATES: u64>(
    Inputs {
        scripts: s,
        creates: c,
        measure,
    }: Inputs,
    i: &mut usize,
    name: &str,
    executor: &mut Executor<MaybeRelayerAdapter>,
    execute: &mut BenchmarkGroup<WallTime>,
) {
    let mut found_solutions = vec![];
    for (id, coin) in coins::<DB_COINS>(1).map(|c| (c.utxo_id, CompressedCoin::from(c))) {
        executor
            .database
            .storage::<Coins>()
            .insert(&id, &coin)
            .unwrap();
    }
    for t in scripts::<SCRIPTS>(s) {
        let param = {
            let inputs =
                fuel_core_types::fuel_tx::field::Inputs::inputs(t.as_script().unwrap())
                    .iter()
                    .map(|i| i.repr())
                    .collect::<Vec<_>>();
            let outputs =
                fuel_core_types::fuel_tx::field::Outputs::outputs(t.as_script().unwrap())
                    .iter()
                    .map(|o| o.repr())
                    .collect::<Vec<_>>();
            format!("Inputs: {inputs:?}, Outputs: {outputs:?}")
        };
        if find_combinations(executor, vec![t.clone()]) {
            found_solutions.push(format!("Script {param}"));
        }
        measure_transactions(
            executor,
            vec![t],
            measure as u64,
            &format!("{name}/{i}"),
            execute,
            &param,
        );
        *i += 1;
    }
    for t in creates::<CREATES>(c) {
        let param = {
            let inputs =
                fuel_core_types::fuel_tx::field::Inputs::inputs(t.as_create().unwrap())
                    .iter()
                    .map(|i| i.repr())
                    .collect::<Vec<_>>();
            let outputs =
                fuel_core_types::fuel_tx::field::Outputs::outputs(t.as_create().unwrap())
                    .iter()
                    .map(|o| o.repr())
                    .collect::<Vec<_>>();
            format!("Inputs: {inputs:?}, Outputs: {outputs:?}")
        };
        if find_combinations(executor, vec![t.clone()]) {
            found_solutions.push(format!("Create {param}"));
        }
        measure_transactions(
            executor,
            vec![t],
            measure as u64,
            &format!("{name}/{i}"),
            execute,
            &param,
        );
        *i += 1;
    }
    if !found_solutions.is_empty() {
        println!("Found solutions for {name}: {found_solutions:?}");
    } else {
        println!("No solutions found for {name}");
    }
}

fn array_32s<const CARDINALITY: u64>() -> impl Iterator<Item = [u8; 32]> + Clone {
    all::<Constrained<CARDINALITY, [u8; 32]>>().map(|c| c.0)
}

fn words<const CARDINALITY: u64>() -> impl Iterator<Item = Word> + Clone {
    all::<Constrained<CARDINALITY, u64>>().map(|c| c.0)
}

fn utxo_ids<const CARDINALITY: u64>(
    output_index: u8,
) -> impl Iterator<Item = UtxoId> + Clone {
    array_32s::<CARDINALITY>().map(move |tx_id| UtxoId::new(tx_id.into(), output_index))
}

fn output_coins<const CARDINALITY: u64>() -> impl Iterator<Item = Output> + Clone {
    let mut to = array_32s::<{ u64::MAX }>().cycle().map(Address::from);
    let mut asset_id = array_32s::<{ u64::MAX }>().map(AssetId::from).cycle();
    let mut amount = words::<CARDINALITY>().cycle();
    (0..CARDINALITY).map(move |_| {
        Output::coin(
            to.next().unwrap(),
            amount.next().unwrap(),
            asset_id.next().unwrap(),
        )
    })
}

fn outputs<const CARDINALITY: u64>() -> impl Iterator<Item = Output> + Clone {
    let mut to = array_32s::<{ u64::MAX }>().cycle().map(Address::from);
    let mut asset_id = array_32s::<{ u64::MAX }>().map(AssetId::from).cycle();
    let mut contract_id = array_32s::<{ u64::MAX }>().map(ContractId::from).cycle();
    let mut amount = words::<CARDINALITY>().cycle();

    let mut input_index = words::<CARDINALITY>().cycle().map(|i| i as u8);
    let mut bytes32 = array_32s::<{ u64::MAX }>().cycle().map(Bytes32::from);
    (0..CARDINALITY).map(move |i| match i % 5 {
        0 => Output::coin(
            to.next().unwrap(),
            amount.next().unwrap(),
            asset_id.next().unwrap(),
        ),
        1 => {
            let b = bytes32.next().unwrap();
            Output::contract(input_index.next().unwrap(), b, b)
        }
        2 => Output::change(
            to.next().unwrap(),
            amount.next().unwrap(),
            asset_id.next().unwrap(),
        ),
        3 => Output::variable(
            to.next().unwrap(),
            amount.next().unwrap(),
            asset_id.next().unwrap(),
        ),
        4 => {
            Output::contract_created(contract_id.next().unwrap(), bytes32.next().unwrap())
        }
        _ => unreachable!(),
    })
}

fn all_outputs() -> impl Iterator<Item = Output> + Clone {
    outputs::<4>()
}

// Has length CARDINALITY^2
fn coins<const CARDINALITY: u64>(
    output_index: u8,
) -> impl Iterator<Item = SignedCoin> + Clone {
    array_32s::<CARDINALITY>()
        .cartesian_product(array_32s::<CARDINALITY>())
        .zip(words::<CARDINALITY>().cycle())
        .zip(array_32s::<{ u64::MAX }>().cycle())
        .map(move |(((secret, asset_id), amount), tx_id)| {
            let secret = secret_key(secret);
            let utxo_id = UtxoId::new(tx_id.into(), output_index);
            SignedCoin {
                secret,
                utxo_id,
                amount,
                asset_id: asset_id.into(),
                tx_pointer: Default::default(),
                maturity: Default::default(),
            }
        })
}

fn messages<const CARDINALITY: u64>(
    message_data_size: usize,
) -> impl Iterator<Item = SignedMessage> + Clone {
    let mut data = bytes::<255>().cycle();
    let mut nonce = array_32s::<{ u64::MAX }>().cycle();
    let mut amount = words::<CARDINALITY>().cycle();
    array_32s::<CARDINALITY>()
        .cartesian_product(array_32s::<CARDINALITY>())
        .map(move |(secret, sender)| {
            let secret = secret_key(secret);
            let recipient = Input::owner(&secret.public_key());
            let data = data.by_ref().take(message_data_size).collect();
            SignedMessage {
                secret,
                recipient,
                amount: amount.next().unwrap(),
                sender: sender.into(),
                nonce: nonce.next().unwrap().into(),
                data,
            }
        })
}

fn bytes<const CARDINALITY: u64>() -> impl Iterator<Item = u8> + Clone {
    all::<u8>().cycle().take(CARDINALITY as usize)
}

fn script_code<const CARDINALITY: u64>(
    size: usize,
) -> impl Iterator<Item = Vec<u8>> + Clone {
    let out: Vec<_> = bytes::<255>()
        .cycle()
        .chunks(size / (CARDINALITY as usize))
        .into_iter()
        .map(|b| {
            let mut out: Vec<u8> = [op::ret(0)].into_iter().collect();
            out.extend(b);
            out
        })
        .take(CARDINALITY as usize)
        .collect();
    out.into_iter()
}

#[derive(Default)]
struct ScriptInputs {
    inputs: Vec<Input>,
    witnesses: Vec<Witness>,
    secrets: Vec<SecretKey>,
}

fn create_inputs_for_coins(
    coins: &mut impl Iterator<Item = SignedCoin>,
    ScriptInputs {
        inputs,
        witnesses,
        secrets,
    }: &mut ScriptInputs,
    coins_per_script: usize,
) {
    for coin in coins.by_ref().take(coins_per_script) {
        let owner = Input::owner(&coin.secret.public_key());
        let witness_idx = witnesses.len() as u8;
        let input = Input::coin_signed(
            coin.utxo_id,
            owner,
            coin.amount,
            coin.asset_id,
            coin.tx_pointer,
            witness_idx,
            coin.maturity,
        );
        inputs.push(input);
        witnesses.push(Witness::default());
        secrets.push(coin.secret);
    }
}

fn create_inputs_for_messages(
    messages: &mut impl Iterator<Item = SignedMessage>,
    ScriptInputs {
        inputs,
        witnesses,
        secrets,
    }: &mut ScriptInputs,
    coins_per_script: usize,
) {
    for message in messages.by_ref().take(coins_per_script) {
        let witness_idx = witnesses.len() as u8;
        let input = if message.data.is_empty() {
            Input::message_data_signed(
                message.sender,
                message.recipient,
                message.amount,
                message.nonce,
                witness_idx,
                message.data,
            )
        } else {
            Input::message_coin_signed(
                message.sender,
                message.recipient,
                message.amount,
                message.nonce,
                witness_idx,
            )
        };
        inputs.push(input);
        witnesses.push(Witness::default());
        secrets.push(message.secret);
    }
}

fn scripts<const CARDINALITY: u64>(
    Scripts {
        coins_per_script,
        messages_per_script,
        script_size,
        script_data_size,
        message_data_size,
        number_extra_witnesses,
        extra_witnesses_data,
        num_outputs,
    }: Scripts,
) -> impl Iterator<Item = Transaction> + Clone {
    let mut coins = coins::<100>(1).cycle();
    let mut messages = messages::<100>(message_data_size).cycle();
    let mut data = bytes::<255>().cycle();
    let mut witnesses_data = bytes::<255>().cycle();
    let mut outputs = all_outputs().cycle();
    script_code::<CARDINALITY>(script_size).map(move |script| {
        let data = data.by_ref().take(script_data_size).collect();
        let mut script_inputs = ScriptInputs::default();

        create_inputs_for_coins(&mut coins, &mut script_inputs, coins_per_script);
        create_inputs_for_messages(
            &mut messages,
            &mut script_inputs,
            messages_per_script,
        );

        let ScriptInputs {
            inputs,
            mut witnesses,
            secrets,
        } = script_inputs;

        let contains_input_contract =
            inputs.iter().any(|i| matches!(i, Input::Contract(_)));

        let outputs = outputs
            .by_ref()
            .filter(|o| {
                match o {
                    Output::Coin { to, amount, asset_id } => true,
                    Output::Contract { input_index, balance_root, state_root } => {
                        contains_input_contract
                    },
                    Output::Change { to, amount, asset_id } => true,
                    Output::Variable { to, amount, asset_id } => true,
                    Output::ContractCreated { contract_id, state_root } => true,
                }
            })
            .take(num_outputs)
            .collect();

        witnesses.extend((0..number_extra_witnesses).map(|_| {
            let data: Vec<_> =
                witnesses_data.by_ref().take(extra_witnesses_data).collect();
            Witness::from(data)
        }));

        let mut script =
            Transaction::script(0, 0, 0.into(), script, data, inputs, outputs, witnesses);
        script.prepare_init_script();

        for secret in secrets {
            script.sign_inputs(&secret, &ConsensusParameters::default());
        }

        script.precompute(&ConsensusParameters::default());
        script.into()
    })
}

fn creates<const CARDINALITY: u64>(
    Creates { num_outputs }: Creates,
) -> impl Iterator<Item = Transaction> + Clone {
    let mut outputs = all_outputs().cycle();
    let mut salt = array_32s::<{ u64::MAX }>().map(Salt::new).cycle();
    (0..CARDINALITY).map(move |_| {
        let outputs = outputs.by_ref().take(num_outputs).collect();
        let create = Transaction::create(
            0,
            0,
            0.into(),
            0,
            salt.next().unwrap(),
            vec![],
            vec![],
            outputs,
            vec![],
        );
        create.into()
    })
}

fn secret_key(seed: [u8; 32]) -> SecretKey {
    let phrase = generate_mnemonic_phrase(&mut StdRng::from_seed(seed), 24).unwrap();
    SecretKey::new_from_mnemonic_phrase_with_path(&phrase, "m/44'/60'/0'/0/0").unwrap()
}

impl From<SignedCoin> for CompressedCoin {
    fn from(coin: SignedCoin) -> Self {
        Self {
            owner: Input::owner(&coin.secret.public_key()),
            amount: coin.amount,
            asset_id: coin.asset_id,
            tx_pointer: coin.tx_pointer,
            maturity: coin.maturity,
        }
    }
}

fn measure_transactions(
    executor: &Executor<MaybeRelayerAdapter>,
    transactions: Vec<Transaction>,
    inputs: u64,
    name: &str,
    execute: &mut BenchmarkGroup<WallTime>,
    params: &str,
) {
    let header = make_header();
    let block = PartialFuelBlock::new(header, transactions);
    let block = ExecutionBlock::Production(block);

    let result = executor.execute_without_commit(block.clone()).unwrap();
    if !result.result().skipped_transactions.is_empty() {
        let status = result.result().tx_status.clone();
        let errors = result
            .result()
            .skipped_transactions
            .iter()
            .map(|(_, e)| e)
            .collect::<Vec<_>>();
        eprintln!("Transaction failed: {params}, {errors:?}, {status:?}");

        return
    }

    execute.throughput(criterion::Throughput::Elements(inputs));
    execute.bench_with_input(BenchmarkId::new(name, inputs), &inputs, |b, _| {
        b.iter(|| {
            let result = executor.execute_without_commit(block.clone()).unwrap();
            assert!(result.result().skipped_transactions.is_empty());
        })
    });
}

fn find_combinations(
    executor: &Executor<MaybeRelayerAdapter>,
    transactions: Vec<Transaction>,
) -> bool {
    let header = make_header();
    let block = PartialFuelBlock::new(header, transactions);
    let block = ExecutionBlock::Production(block);

    executor
        .execute_without_commit(block)
        .unwrap()
        .result()
        .skipped_transactions
        .is_empty()
}

fn make_header() -> PartialBlockHeader {
    PartialBlockHeader {
        application: ApplicationHeader {
            da_height: 1u64.into(),
            generated: Default::default(),
        },
        consensus: ConsensusHeader {
            prev_root: Bytes32::zeroed(),
            height: 1u32.into(),
            time: fuel_core_types::tai64::Tai64::now(),
            generated: Default::default(),
        },
    }
}

criterion_group!(benches, txn);
criterion_main!(benches);

#[derive(Clone, Debug)]
struct Constrained<const CARDINALITY: u64, T>(T)
where
    T: Cardinality<CARDINALITY>;

trait Cardinality<const CARDINALITY: u64> {
    fn current(&self) -> u64;
    fn from_u64(value: u64) -> Self;
}

impl<const CARDINALITY: u64> Cardinality<CARDINALITY> for u64 {
    fn current(&self) -> u64 {
        *self
    }

    fn from_u64(value: u64) -> Self {
        value
    }
}

impl<const CARDINALITY: u64> Cardinality<CARDINALITY> for [u8; 32] {
    fn current(&self) -> u64 {
        u64::from_be_bytes(self[..std::mem::size_of::<u64>()].try_into().unwrap())
    }

    fn from_u64(value: u64) -> Self {
        let mut bytes = [0u8; 32];
        bytes[..std::mem::size_of::<u64>()].copy_from_slice(&value.to_be_bytes());
        bytes
    }
}

impl<const CARDINALITY: u64, T> Sequence for Constrained<CARDINALITY, T>
where
    T: Cardinality<CARDINALITY>,
{
    const CARDINALITY: usize = CARDINALITY as usize;

    fn next(&self) -> Option<Self> {
        let current = self.0.current();
        let Some(next) = ConstrainedU64::<CARDINALITY>(current).next() else { return None };
        Some(Self(<T>::from_u64(next.0)))
    }

    fn previous(&self) -> Option<Self> {
        let Some(prev) = ConstrainedU64::<CARDINALITY>(self.0.current()).previous() else { return None };
        Some(Self(<T>::from_u64(prev.0)))
    }

    fn first() -> Option<Self> {
        Some(Self(<T>::from_u64(0)))
    }

    fn last() -> Option<Self> {
        let Some(last) = CARDINALITY.checked_sub(1) else { return None };
        Some(Self(<T>::from_u64(last)))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct ConstrainedU64<const CARDINALITY: u64>(u64);

impl<const CARDINALITY: u64> Sequence for ConstrainedU64<CARDINALITY> {
    const CARDINALITY: usize = CARDINALITY as usize;

    fn next(&self) -> Option<Self> {
        let next = self.0.checked_add(1)?;
        if next < CARDINALITY {
            Some(Self(next))
        } else {
            None
        }
    }

    fn previous(&self) -> Option<Self> {
        Some(Self(self.0.checked_sub(1)?))
    }

    fn first() -> Option<Self> {
        Some(Self(u64::MIN))
    }

    fn last() -> Option<Self> {
        Some(Self(CARDINALITY.checked_sub(1)?))
    }
}
