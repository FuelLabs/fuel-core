use criterion::{
    criterion_group,
    criterion_main,
    Criterion,
};
use fuel_core_benches::db_lookup_times_utils::{
    matrix::matrix,
    seed::{
        seed_compressed_blocks_and_transactions_matrix,
        seed_full_block_matrix,
    },
    utils::{
        get_random_block_height,
        open_rocks_db,
        LookupMethod,
        Result as DbLookupBenchResult,
    },
};

use fuel_core_benches::utils::ShallowTempDir;
use rand::thread_rng;

pub fn header_and_tx_lookup(c: &mut Criterion) -> DbLookupBenchResult<()> {
    let method = LookupMethod::HeaderAndTx;
    let mut rng = thread_rng();

    let mut group = c.benchmark_group(method.as_ref());

    for (block_count, tx_count) in matrix() {
        let db_path = ShallowTempDir::new();
        let mut database = open_rocks_db(db_path.path())?;
        seed_compressed_blocks_and_transactions_matrix(
            &mut database,
            block_count,
            tx_count,
        )?;

        group.bench_function(format!("{block_count}/{tx_count}"), |b| {
            b.iter(|| {
                let height = get_random_block_height(&mut rng, block_count);
                let block = method.get_block(&database, height);
                assert!(block.is_ok());
            });
        });
    }

    group.finish();
    Ok(())
}

pub fn multi_get_lookup(c: &mut Criterion) -> DbLookupBenchResult<()> {
    let method = LookupMethod::MultiGet;
    let mut rng = thread_rng();

    let mut group = c.benchmark_group(method.as_ref());

    for (block_count, tx_count) in matrix() {
        let db_path = ShallowTempDir::new();
        let mut database = open_rocks_db(db_path.path())?;
        seed_compressed_blocks_and_transactions_matrix(
            &mut database,
            block_count,
            tx_count,
        )?;

        group.bench_function(format!("{block_count}/{tx_count}"), |b| {
            b.iter(|| {
                let height = get_random_block_height(&mut rng, block_count);
                let block = method.get_block(&database, height);
                assert!(block.is_ok());
            });
        });
    }

    group.finish();
    Ok(())
}

pub fn full_block_lookup(c: &mut Criterion) -> DbLookupBenchResult<()> {
    let method = LookupMethod::FullBlock;
    let mut rng = thread_rng();

    let mut group = c.benchmark_group(method.as_ref());

    for (block_count, tx_count) in matrix() {
        let db_path = ShallowTempDir::new();
        let mut database = open_rocks_db(db_path.path())?;
        seed_full_block_matrix(&mut database, block_count, tx_count)?;

        group.bench_function(format!("{block_count}/{tx_count}"), |b| {
            b.iter(|| {
                let height = get_random_block_height(&mut rng, block_count);
                let full_block = method.get_block(&database, height);
                assert!(full_block.is_ok());
            });
        });
    }

    group.finish();
    Ok(())
}

fn construct_and_run_benchmarks(c: &mut Criterion) {
    header_and_tx_lookup(c).unwrap();
    multi_get_lookup(c).unwrap();
    full_block_lookup(c).unwrap();
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10).measurement_time(std::time::Duration::from_secs(10));
    targets = construct_and_run_benchmarks
}
criterion_main!(benches);
