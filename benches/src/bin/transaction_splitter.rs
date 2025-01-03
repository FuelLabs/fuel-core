use fuel_core::{
    parallel_executor::dependency_splitter::DependencySplitter,
    upgradable_executor::native_executor::ports::MaybeCheckedTransaction,
};
use fuel_core_types::{
    fuel_asm::{
        op,
        GMArgs,
        GTFArgs,
        RegId,
    },
    fuel_crypto::{
        coins_bip32::ecdsa::signature::Signer,
        *,
    },
    fuel_tx::{
        AssetId,
        Cacheable,
        ConsensusParameters,
        Finalizable,
        Input,
        Output,
        Transaction,
        TransactionBuilder,
    },
    fuel_types::{
        Immediate12,
        Immediate18,
    },
    fuel_vm::{
        checked_transaction::{
            CheckPredicateParams,
            EstimatePredicates,
        },
        interpreter::MemoryInstance,
        predicate::EmptyStorage,
    },
};
use fuel_types::ChainId;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use std::num::NonZeroUsize;
use test_helpers::builder::local_chain_config;

#[cfg(feature = "parallel-executor")]
fn main() {
    let n = std::env::var("BENCH_TXS_NUMBER")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap();
    let number_of_cores = std::env::var("FUEL_BENCH_CORES")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap();
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322u64);
    let mut consensus_parameters = ConsensusParameters::default();
    consensus_parameters.set_block_gas_limit(u64::MAX);
    let chain_id = consensus_parameters.chain_id();
    let transactions = generate_transactions(n, &mut rng, &chain_id);
    let txs_len = transactions.len();
    let splitter = DependencySplitter::new(consensus_parameters.clone(), txs_len);
    bench(splitter, transactions, number_of_cores, &chain_id);
}

fn bench(
    mut splitter: DependencySplitter,
    transactions: Vec<MaybeCheckedTransaction>,
    number_of_cores: usize,
    chain_id: &ChainId,
) {
    let start = std::time::Instant::now();
    for tx in transactions {
        let tx_id = tx.id(&chain_id);
        splitter.process(tx, tx_id).unwrap();
    }
    tracing::info!(
        "Processing took {:?} milliseconds",
        start.elapsed().as_millis()
    );
    let start = std::time::Instant::now();
    splitter.split_equally(NonZeroUsize::new(number_of_cores).unwrap());
    tracing::info!(
        "Splitting took {:?} milliseconds",
        start.elapsed().as_millis()
    );
}

fn checked_parameters() -> CheckPredicateParams {
    local_chain_config().consensus_parameters.into()
}

fn generate_transactions(
    nb_txs: u64,
    rng: &mut StdRng,
    chain_id: &ChainId,
) -> Vec<MaybeCheckedTransaction> {
    let mut transactions = Vec::with_capacity(nb_txs as usize);
    for _ in 0..nb_txs {
        let ed19_secret = ed25519_dalek::SigningKey::generate(rng);
        let public = ed19_secret.verifying_key();

        let message = b"The gift of words is the gift of deception and illusion.";
        let message = Message::new(message);

        let signature = ed19_secret.sign(&*message).to_bytes();

        let predicate = vec![
            op::gm_args(0x20, GMArgs::GetVerifyingPredicate),
            op::gtf_args(0x20, 0x20, GTFArgs::InputCoinPredicateData),
            op::addi(0x21, 0x20, PublicKey::LEN as Immediate12),
            op::addi(0x22, 0x21, signature.len() as Immediate12),
            op::movi(0x24, message.as_ref().len() as Immediate18),
            op::ed19(0x20, 0x21, 0x22, 0x24),
            op::eq(0x12, RegId::ERR, RegId::ONE),
            op::ret(0x12),
        ]
        .into_iter()
        .collect::<Vec<u8>>();
        let owner = Input::predicate_owner(&predicate);

        let predicate_data: Vec<u8> = public
            .to_bytes()
            .iter()
            .copied()
            .chain(
                signature
                    .iter()
                    .copied()
                    .chain(message.as_ref().iter().copied()),
            )
            .collect();

        let mut tx = TransactionBuilder::script(vec![], vec![])
            .script_gas_limit(10000)
            .add_input(Input::coin_predicate(
                rng.gen(),
                owner,
                1000,
                Default::default(),
                Default::default(),
                Default::default(),
                predicate.clone(),
                predicate_data.clone(),
            ))
            .add_output(Output::coin(rng.gen(), 50, AssetId::default()))
            .finalize();
        tx.estimate_predicates(
            &checked_parameters(),
            MemoryInstance::new(),
            &EmptyStorage,
        )
        .expect("Predicate check failed");
        let mut tx: Transaction = tx.into();
        tx.precompute(chain_id).unwrap();
        transactions.push(MaybeCheckedTransaction::Transaction(tx));
    }
    transactions
}

#[cfg(not(feature = "parallel-executor"))]
fn main() {
    panic!("This binary requires the `parallel-executor` feature to be enabled");
}

fuel_core_trace::enable_tracing!();
