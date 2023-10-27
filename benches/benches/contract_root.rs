use criterion::{
    criterion_group,
    criterion_main,
    Criterion,
};
use fuel_core_types::fuel_tx::Contract;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};

// 17 mb contract size
const MAX_CONTRACT_SIZE: usize = 17 * 1024 * 1024;

fn random_bytes<const N: usize, R: Rng + ?Sized>(rng: &mut R) -> Box<[u8; N]> {
    let mut bytes = Box::new([0u8; N]);
    for chunk in bytes.chunks_mut(32) {
        rng.fill(chunk);
    }
    bytes
}

pub fn contract_root(c: &mut Criterion) {
    let rng = &mut StdRng::seed_from_u64(8586);

    let mut group = c.benchmark_group("contract_root");

    let bytes = random_bytes::<MAX_CONTRACT_SIZE, _>(rng);
    group.bench_function("root_from_bytecode", |b| {
        b.iter(|| {
            let contract = Contract::from(bytes.as_slice());
            contract.root();
        })
    });

    group.finish();
}
