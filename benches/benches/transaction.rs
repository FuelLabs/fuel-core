use criterion::{
    black_box,
    criterion_group,
    criterion_main,
    Criterion,
};
use fuel_core::{
    executor::Executor,
    service::{
        adapters::MaybeRelayerAdapter,
        Config,
    },
};
use fuel_core_benches::{
    Database,
    Rng,
};
use fuel_core_storage::{
    tables::Coins,
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
    entities::coin::CompressedCoin,
    fuel_asm::op,
    fuel_tx::{
        Finalizable,
        Input,
        TransactionBuilder,
        UtxoId,
    },
    fuel_types::Bytes32,
    fuel_vm::SecretKey,
    services::executor::ExecutionBlock,
};
use rand::{
    rngs::StdRng,
    SeedableRng,
};

fn txn(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(1234);

    let secret_key: SecretKey = rng.gen();

    let coin_utxo: UtxoId = rng.gen();

    let compressed_coin = CompressedCoin {
        owner: Input::owner(&secret_key.public_key()),
        amount: rng.gen(),
        asset_id: rng.gen(),
        tx_pointer: Default::default(),
        maturity: 0u32.into(),
    };

    let mut database = Database::default();
    database
        .storage::<Coins>()
        .insert(&coin_utxo, &compressed_coin)
        .unwrap();

    let relayer = MaybeRelayerAdapter {
        database: database.clone(),
    };
    let mut config = Config::local_node();
    config.utxo_validation = true;
    let executor = Executor {
        database,
        relayer,
        config,
    };
    let header = PartialBlockHeader {
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
    };

    let script = TransactionBuilder::script(
        vec![op::noop(), op::ret(0)].into_iter().collect(),
        vec![],
    )
    .add_unsigned_coin_input(
        secret_key,
        coin_utxo,
        compressed_coin.amount,
        compressed_coin.asset_id,
        Default::default(),
        0,
    )
    .finalize();

    let transactions = vec![script.into()];
    let block = PartialFuelBlock::new(header, transactions);
    let block = ExecutionBlock::Production(block);
    c.bench_function("executor::execute", |b| {
        b.iter(|| black_box(executor.execute_without_commit(block.clone())).unwrap())
    });
}

criterion_group!(benches, txn);
criterion_main!(benches);
