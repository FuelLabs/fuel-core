use crate::utils::aloc_bytearray;

use super::run_group_ref;

use criterion::{
    Criterion,
    Throughput,
};
use ed25519_dalek::Signer;
use fuel_core_benches::*;
use fuel_core_types::{
    fuel_asm::*,
    fuel_crypto::*,
};
use rand::{
    rngs::StdRng,
    SeedableRng,
};

pub fn run(c: &mut Criterion) {
    let rng = &mut StdRng::seed_from_u64(2322u64);

    let message = Message::new(b"foo");

    let eck1_secret = SecretKey::random(rng);
    let eck1_signature = Signature::sign(&eck1_secret, &message);

    run_group_ref(
        &mut c.benchmark_group("eck1"),
        "eck1",
        VmBench::new(op::eck1(RegId::HP, 0x20, 0x21))
            .with_prepare_script(vec![
                op::gtf_args(0x20, 0x00, GTFArgs::ScriptData),
                op::addi(
                    0x21,
                    0x20,
                    eck1_signature.as_ref().len().try_into().unwrap(),
                ),
                op::movi(0x10, PublicKey::LEN.try_into().unwrap()),
                op::aloc(0x10),
            ])
            .with_data(
                eck1_signature
                    .iter()
                    .chain(message.iter())
                    .copied()
                    .collect(),
            ),
    );

    let ecr1_secret = p256::ecdsa::SigningKey::random(rng);
    let ecr1_signature = secp256r1::sign_prehashed(&ecr1_secret, &message)
        .expect("Failed to sign with secp256r1");

    run_group_ref(
        &mut c.benchmark_group("ecr1"),
        "ecr1",
        VmBench::new(op::ecr1(RegId::HP, 0x20, 0x21))
            .with_prepare_script(vec![
                op::gtf_args(0x20, 0x00, GTFArgs::ScriptData),
                op::addi(
                    0x21,
                    0x20,
                    ecr1_signature.as_ref().len().try_into().unwrap(),
                ),
                op::movi(0x10, PublicKey::LEN.try_into().unwrap()),
                op::aloc(0x10),
            ])
            .with_data(
                ecr1_signature
                    .iter()
                    .chain(message.iter())
                    .copied()
                    .collect(),
            ),
    );

    let linear = super::utils::arb_dependent_cost_values();

    let mut bench_k256 = c.benchmark_group("k256");
    for i in &linear {
        bench_k256.throughput(Throughput::Bytes(*i as u64));
        run_group_ref(
            &mut bench_k256,
            format!("{i}"),
            VmBench::new(op::k256(RegId::HP, RegId::ZERO, 0x10)).with_prepare_script(
                vec![
                    op::movi(0x11, 32),
                    op::aloc(0x11),
                    op::movi(0x10, *i),
                    op::cfe(0x10),
                ],
            ),
        );
    }
    bench_k256.finish();

    let mut bench_s256 = c.benchmark_group("s256");
    for i in &linear {
        bench_s256.throughput(Throughput::Bytes(*i as u64));
        run_group_ref(
            &mut bench_s256,
            format!("{i}"),
            VmBench::new(op::s256(RegId::HP, RegId::ZERO, 0x10)).with_prepare_script(
                vec![
                    op::movi(0x11, 32),
                    op::aloc(0x11),
                    op::movi(0x10, *i),
                    op::cfe(0x10),
                ],
            ),
        );
    }
    bench_s256.finish();

    let ed19_secret = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng {});
    let ed19_signature = ed19_secret.sign(&*message);

    let mut bench_ed19 = c.benchmark_group("ed19");
    for i in &linear {
        bench_ed19.throughput(Throughput::Bytes(*i as u64));
        run_group_ref(
            &mut bench_ed19,
            format!("{i}"),
            VmBench::new(op::ed19(0x20, 0x21, RegId::ZERO, 0x10))
                .with_prepare_script(vec![
                    op::gtf_args(0x20, 0x00, GTFArgs::ScriptData),
                    op::addi(
                        0x21,
                        0x20,
                        ed19_secret
                            .verifying_key()
                            .as_bytes()
                            .len()
                            .try_into()
                            .unwrap(),
                    ),
                    op::addi(
                        0x22,
                        0x21,
                        ed19_signature.to_bytes().len().try_into().unwrap(),
                    ),
                    op::movi(0x10, *i),
                    op::cfe(0x10),
                ])
                .with_data(
                    ed19_secret
                        .verifying_key()
                        .to_bytes()
                        .iter()
                        .chain(ed19_signature.to_bytes().iter())
                        .chain(message.iter())
                        .copied()
                        .collect(),
                ),
        );
    }
    bench_ed19.finish();

    // ecop testing mul as it's the most expensive operation
    let mut points_bytearray = Vec::new();
    // X
    points_bytearray.extend(
        hex::decode("2bd3e6d0f3b142924f5ca7b49ce5b9d54c4703d7ae5648e61d02268b1a0a9fb7")
            .unwrap(),
    );
    // Y
    points_bytearray.extend(
        hex::decode("21611ce0a6af85915e2f1d70300909ce2e49dfad4a4619c8390cae66cefdb204")
            .unwrap(),
    );
    // Scalar
    points_bytearray.extend(
        hex::decode("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff")
            .unwrap(),
    );
    // 96 bytes = 1 point and 1 scalar
    let prepare_points = aloc_bytearray::<96>(0x10, points_bytearray.try_into().unwrap());
    run_group_ref(
        &mut c.benchmark_group("ecop"),
        "ecop",
        VmBench::new(op::ecop(0x10, 0x00, 0x01, 0x10))
            .with_prepare_script(prepare_points),
    );

    // TODO: Update with real values
    // ec pairing
    // run_group_ref(
    //     &mut c.benchmark_group("ecpair"),
    //     "epar",
    //     VmBench::new(op::epar(1, 1, 1, 1)),
    // );
}
