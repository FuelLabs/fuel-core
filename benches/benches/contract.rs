use criterion::{
    Criterion,
    Throughput,
};
use fuel_core_types::fuel_tx::{
    Contract,
    StorageSlot,
};
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use std::iter::successors;

fn random_bytes<R: Rng + ?Sized>(n: usize, rng: &mut R) -> Vec<u8> {
    let mut bytes = vec![0; n];
    for chunk in bytes.chunks_mut(32) {
        rng.fill(chunk);
    }
    bytes
}

pub fn contract_root(c: &mut Criterion) {
    let rng = &mut StdRng::seed_from_u64(8586);

    let mut group = c.benchmark_group("contract_root");

    // Let MAX_CONTRACT_SIZE be the maximum size of a contract's bytecode.
    // Because contract root calculation is guaranteed to have a logarithmic
    // complexity, we can test exponentially increasing data inputs up to
    // MAX_CONTRACT_SIZE to provide a meaningful model of linear growth.
    // If MAX_CONTRACT_SIZE = 17 mb = 17 * 1024 * 1024 b = 2^24 b + 2^20 b, we
    // can sufficiently cover this range by testing up to 2^25 b, given that
    // 2^24 < MAX_CONTRACT_SIZE < 2^25.
    const N: usize = 25;
    let sizes = successors(Some(2), |n| Some(n * 2)).take(N);
    for (i, size) in sizes.enumerate() {
        let bytes = random_bytes(size, rng);
        group.throughput(Throughput::Bytes(size as u64));
        let name = format!("root_from_bytecode_size_2^{exp:#02}", exp = i + 1);
        group.bench_function(name, |b| {
            b.iter(|| {
                let contract = Contract::from(bytes.as_slice());
                contract.root();
            })
        });
    }

    group.finish();
}

pub fn state_root(c: &mut Criterion) {
    let rng = &mut StdRng::seed_from_u64(8586);

    let mut group = c.benchmark_group("state_root");

    const N: usize = 20;
    let sizes = successors(Some(2), |n| Some(n * 2)).take(N);
    for (i, size) in sizes.enumerate() {
        let gen_storage_slot = || rng.gen::<StorageSlot>();
        let storage_slots = std::iter::repeat_with(gen_storage_slot)
            .take(size)
            .collect::<Vec<_>>();
        group.throughput(Throughput::Bytes(size as u64));
        let name = format!("state_root_from_slots_2^{exp:#02}", exp = i + 1);
        group.bench_function(name, |b| {
            b.iter(|| {
                Contract::initial_state_root(storage_slots.iter());
            })
        });
    }

    group.finish();
}
