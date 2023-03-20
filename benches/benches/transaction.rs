use std::iter;

use criterion::{
    black_box,
    criterion_group,
    criterion_main,
    BenchmarkId,
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

    let mut database = Database::default();
    let relayer = MaybeRelayerAdapter {
        database: database.clone(),
        // relayer_synced: None,
        // da_deploy_height: 0u64.into(),
    };
    let mut config = Config::local_node();

    let coins: Vec<_> = iter::repeat_with(|| {
        let coin_utxo: UtxoId = rng.gen();

        let compressed_coin = CompressedCoin {
            owner: Input::owner(&secret_key.public_key()),
            amount: rng.gen(),
            asset_id: rng.gen(),
            tx_pointer: Default::default(),
            maturity: 0u32.into(),
        };
        (coin_utxo, compressed_coin)
    })
    .take(100)
    .collect();

    for (coin_utxo, compressed_coin) in coins.iter() {
        database
            .storage::<Coins>()
            .insert(coin_utxo, compressed_coin)
            .unwrap();
    }

    config.utxo_validation = true;
    let executor = Executor {
        database,
        relayer,
        config,
    };

    let mut execute = c.benchmark_group("execute_without_commit");

    for num_inputs in [0, 1, 2, 5, 10, 50, 100] {
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

        let mut script = TransactionBuilder::script(
            vec![op::noop(), op::ret(0)].into_iter().collect(),
            vec![],
        );
        for (coin_utxo, compressed_coin) in coins.iter().take(num_inputs) {
            script.add_unsigned_coin_input(
                secret_key,
                *coin_utxo,
                compressed_coin.amount,
                compressed_coin.asset_id,
                Default::default(),
                0,
            );
        }
        let script = script.finalize();

        let transactions = vec![script.into()];
        let block = PartialFuelBlock::new(header, transactions);
        let block = ExecutionBlock::Production(block);
        execute.throughput(criterion::Throughput::Elements(num_inputs as u64));
        execute.bench_with_input(
            BenchmarkId::from_parameter(num_inputs),
            &num_inputs,
            |b, _| {
                b.iter(|| {
                    black_box(executor.execute_without_commit(block.clone())).unwrap()
                })
            },
        );
    }
}

criterion_group!(benches, txn);
criterion_main!(benches);
