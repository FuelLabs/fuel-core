use super::run_group_ref;

use criterion::Criterion;
use ed25519_dalek::Signer;
use fuel_core_benches::*;
use fuel_core_types::{
    fuel_asm::*,
    fuel_crypto::*,
    fuel_types::*,
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

    run_group_ref(
        &mut c.benchmark_group("k256"),
        "k256",
        VmBench::new(op::k256(RegId::HP, RegId::ZERO, 0x11))
            .with_prepare_script(vec![
                op::movi(0x10, Bytes32::LEN.try_into().unwrap()),
                op::aloc(0x10),
                op::movi(0x11, 32),
            ])
            .with_data(
                eck1_signature
                    .iter()
                    .chain(message.iter())
                    .copied()
                    .collect(),
            ),
    );

    run_group_ref(
        &mut c.benchmark_group("s256"),
        "s256",
        VmBench::new(op::s256(RegId::HP, RegId::ZERO, 0x11))
            .with_prepare_script(vec![
                op::movi(0x10, Bytes32::LEN.try_into().unwrap()),
                op::aloc(0x10),
                op::movi(0x11, 32),
            ])
            .with_data(
                eck1_signature
                    .iter()
                    .chain(message.iter())
                    .copied()
                    .collect(),
            ),
    );

    let ed19_keypair =
        ed25519_dalek::Keypair::generate(&mut ed25519_dalek_old_rand::rngs::OsRng {});
    let ed19_signature = ed19_keypair.sign(&*message);

    run_group_ref(
        &mut c.benchmark_group("ed19"),
        "ed19",
        VmBench::new(op::ed19(0x20, 0x21, 0x22))
            .with_prepare_script(vec![
                op::gtf_args(0x20, 0x00, GTFArgs::ScriptData),
                op::addi(
                    0x21,
                    0x20,
                    ed19_keypair.public.as_ref().len().try_into().unwrap(),
                ),
                op::addi(
                    0x22,
                    0x21,
                    ed19_signature.as_ref().len().try_into().unwrap(),
                ),
            ])
            .with_data(
                ed19_keypair
                    .public
                    .to_bytes()
                    .iter()
                    .chain(ed19_signature.to_bytes().iter())
                    .chain(message.iter())
                    .copied()
                    .collect(),
            ),
    );
}
