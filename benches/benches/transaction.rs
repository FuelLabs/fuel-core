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
use fuel_core_benches::Database;
use fuel_core_types::{
    blockchain::{
        block::PartialFuelBlock,
        header::{
            ApplicationHeader,
            ConsensusHeader,
            PartialBlockHeader,
        },
    },
    fuel_asm::op,
    fuel_tx::{
        Finalizable,
        TransactionBuilder,
    },
    fuel_types::Bytes32,
    services::executor::ExecutionBlock,
};

fn txn(c: &mut Criterion) {
    let database = Database::default();
    let relayer = MaybeRelayerAdapter {
        database: database.clone(),
    };
    let executor = Executor {
        database,
        relayer,
        config: Config::local_node(),
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
    .finalize();
    let transactions = vec![script.into()];
    let block = PartialFuelBlock::new(header, transactions);
    let block = ExecutionBlock::Production(block);
    c.bench_function("executor::execute", |b| {
        b.iter(|| {
            black_box(executor.execute_without_commit(block.clone())).unwrap();
        })
    });
}

criterion_group!(benches, txn);
criterion_main!(benches);
