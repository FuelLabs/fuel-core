use crate::db_lookup_times_utils::{
    matrix::matrix,
    utils::{
        get_full_block,
        get_random_block_height,
        multi_get_block,
        open_db,
        open_raw_rocksdb,
        thread_rng,
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
use fuel_core_storage::transactional::AtomicView;

mod db_lookup_times_utils;

pub fn header_and_tx_lookup(c: &mut Criterion) {
    let method = "header_and_tx";
    let mut rng = thread_rng();

    seed_compressed_blocks_and_transactions_matrix(method);
    let mut group = c.benchmark_group(method);

    for (block_count, tx_count) in matrix() {
        let database = open_db(block_count, tx_count, method);
        let view = database.latest_view().unwrap();
        group.bench_function(format!("{block_count}/{tx_count}"), |b| {
            b.iter(|| {
                let height = get_random_block_height(&mut rng, block_count);
                let block = (&view).get_full_block(&height);
                assert!(block.is_ok());
                assert!(block.unwrap().is_some());
            });
        });
    }

    group.finish();
}

pub fn multi_get_lookup(c: &mut Criterion) {
    let method = "multi_get";
    let mut rng = thread_rng();

    seed_compressed_blocks_and_transactions_matrix(method);
    let mut group = c.benchmark_group(method);

    for (block_count, tx_count) in matrix() {
        let database = open_raw_rocksdb(block_count, tx_count, method);
        group.bench_function(format!("{block_count}/{tx_count}"), |b| {
            b.iter(|| {
                let height = get_random_block_height(&mut rng, block_count);
                // todo: clean up
                multi_get_block(&database, height);
            });
        });
    }

    group.finish();
}

pub fn full_block_lookup(c: &mut Criterion) {
    let method = "full_block";
    let mut rng = thread_rng();

    seed_full_block_matrix();
    let mut group = c.benchmark_group(method);

    for (block_count, tx_count) in matrix() {
        let database = open_db(block_count, tx_count, method);
        let view = database.latest_view().unwrap();
        group.bench_function(format!("{block_count}/{tx_count}"), |b| {
            b.iter(|| {
                let height = get_random_block_height(&mut rng, block_count);
                let full_block = get_full_block(&view, &height);
                assert!(full_block.is_ok());
                assert!(full_block.unwrap().is_some());
            });
        });
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10).measurement_time(std::time::Duration::from_secs(10));
    targets = header_and_tx_lookup, multi_get_lookup, full_block_lookup
}
criterion_main!(benches);
