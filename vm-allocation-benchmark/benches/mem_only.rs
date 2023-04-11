use criterion::{
    black_box,
    criterion_group,
    criterion_main,
    Criterion,
};

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

fn bench(c: &mut Criterion) {
    let mut mem: Box<[u8; 64 * 1024 * 1024]> =
        vec![1u8; 64 * 1024 * 1024].try_into().unwrap();

    c.bench_function("clone", |b| {
        b.iter(|| {
            let _ = black_box(mem.clone());
        })
    });
    c.bench_function("zero", |b| {
        b.iter(|| {
            mem.fill(0);
            black_box(&mem);
        })
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);