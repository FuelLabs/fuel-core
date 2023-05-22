use criterion::{
    black_box,
    criterion_group,
    criterion_main,
    BenchmarkId,
    Criterion,
};

fn rocks(c: &mut Criterion) {}

criterion_group!(benches, rocks);
criterion_main!(benches);
