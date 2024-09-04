use crate::db_lookup_times_utils::{
    full_block_table::BenchDatabase,
    matrix::{
        matrix,
        should_clean,
    },
    utils::{
        get_block_full_block_method,
        get_block_headers_and_tx_method,
        get_block_multi_get_method,
        get_random_block_height,
        open_rocks_db,
        Result as DbLookupBenchResult,
    },
};
use criterion::{
    criterion_group,
    criterion_main,
    Criterion,
};
use db_lookup_times_utils::seed::{
    seed_compressed_blocks_and_transactions_matrix,
    seed_full_block_matrix,
};
use rand::thread_rng;

mod db_lookup_times_utils;

pub fn header_and_tx_lookup(c: &mut Criterion) -> DbLookupBenchResult<impl FnOnce()> {
    let method = "header_and_tx";
    let mut rng = thread_rng();

    let cleaner = seed_compressed_blocks_and_transactions_matrix(method)?;
    let mut group = c.benchmark_group(method);

    for (block_count, tx_count) in matrix() {
        let database = open_rocks_db::<BenchDatabase>(block_count, tx_count, method)?;
        group.bench_function(format!("{block_count}/{tx_count}"), |b| {
            b.iter(|| {
                let height = get_random_block_height(&mut rng, block_count);
                let block = get_block_headers_and_tx_method(&database, height);
                assert!(block.is_ok());
            });
        });
    }

    group.finish();
    Ok(cleaner)
}

pub fn multi_get_lookup(c: &mut Criterion) -> DbLookupBenchResult<impl FnOnce()> {
    let method = "multi_get";
    let mut rng = thread_rng();

    let cleaner = seed_compressed_blocks_and_transactions_matrix(method)?;
    let mut group = c.benchmark_group(method);

    for (block_count, tx_count) in matrix() {
        let database = open_rocks_db(block_count, tx_count, method)?;
        group.bench_function(format!("{block_count}/{tx_count}"), |b| {
            b.iter(|| {
                let height = get_random_block_height(&mut rng, block_count);
                assert!(get_block_multi_get_method(&database, height).is_ok());
            });
        });
    }

    group.finish();
    Ok(cleaner)
}

pub fn full_block_lookup(c: &mut Criterion) -> DbLookupBenchResult<impl FnOnce()> {
    let method = "full_block";
    let mut rng = thread_rng();

    let cleaner = seed_full_block_matrix()?;
    let mut group = c.benchmark_group(method);

    for (block_count, tx_count) in matrix() {
        let database = open_rocks_db(block_count, tx_count, method)?;
        group.bench_function(format!("{block_count}/{tx_count}"), |b| {
            b.iter(|| {
                let height = get_random_block_height(&mut rng, block_count);
                let full_block = get_block_full_block_method(&database, &height);
                assert!(full_block.is_ok());
                assert!(full_block.unwrap().is_some());
            });
        });
    }

    group.finish();
    Ok(cleaner)
}

fn construct_and_run_benchmarks(c: &mut Criterion) {
    let header_and_tx_cleaner = header_and_tx_lookup(c).unwrap();
    let multi_get_cleaner = multi_get_lookup(c).unwrap();
    let full_block_cleaner = full_block_lookup(c).unwrap();

    if should_clean() {
        header_and_tx_cleaner();
        multi_get_cleaner();
        full_block_cleaner();
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10).measurement_time(std::time::Duration::from_secs(10));
    targets = construct_and_run_benchmarks
}
criterion_main!(benches);
