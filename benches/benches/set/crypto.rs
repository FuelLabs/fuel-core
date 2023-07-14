use super::run_group_ref;

use criterion::Criterion;
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

    let secret = SecretKey::random(rng);
    let message = Message::new(b"foo");
    let signature = Signature::sign(&secret, &message);

    run_group_ref(
        &mut c.benchmark_group("eck1"),
        "eck1",
        VmBench::new(op::eck1(0x11, 0x20, 0x21))
            .with_prepare_script(vec![
                op::gtf_args(0x20, 0x00, GTFArgs::ScriptData),
                op::addi(0x21, 0x20, signature.as_ref().len().try_into().unwrap()),
                op::addi(0x22, 0x21, message.as_ref().len().try_into().unwrap()),
                op::movi(0x10, PublicKey::LEN.try_into().unwrap()),
                op::aloc(0x10),
                op::move_(0x11, RegId::HP),
            ])
            .with_data(signature.iter().chain(message.iter()).copied().collect()),
    );

    run_group_ref(
        &mut c.benchmark_group("s256"),
        "s256",
        VmBench::new(op::s256(0x10, 0x00, 0x11))
            .with_prepare_script(vec![
                op::movi(0x10, Bytes32::LEN.try_into().unwrap()),
                op::aloc(0x10),
                op::move_(0x10, RegId::HP),
                op::movi(0x11, 32),
            ])
            .with_data(signature.iter().chain(message.iter()).copied().collect()),
    );

    run_group_ref(
        &mut c.benchmark_group("k256"),
        "k256",
        VmBench::new(op::k256(0x10, 0x00, 0x11))
            .with_prepare_script(vec![
                op::movi(0x10, Bytes32::LEN.try_into().unwrap()),
                op::aloc(0x10),
                op::move_(0x10, RegId::HP),
                op::movi(0x11, 32),
            ])
            .with_data(signature.iter().chain(message.iter()).copied().collect()),
    );
}
