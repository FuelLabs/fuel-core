use super::run_group;

use criterion::Criterion;
use fuel_core_benches::*;
use fuel_core_types::{
    fuel_asm::*,
    fuel_crypto::*,
    fuel_types::*,
    fuel_vm::consts::*,
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

    let mut group = c.benchmark_group("crypto");

    run_group(
        &mut group,
        "ecr",
        VmBench::new(Opcode::ECR(0x11, 0x20, 0x21))
            .with_prepare_script(vec![
                Opcode::gtf(0x20, 0x00, GTFArgs::ScriptData),
                Opcode::ADDI(0x21, 0x20, signature.as_ref().len() as Immediate12),
                Opcode::ADDI(0x22, 0x21, message.as_ref().len() as Immediate12),
                Opcode::MOVI(0x10, PublicKey::LEN as Immediate18),
                Opcode::ALOC(0x10),
                Opcode::ADDI(0x11, REG_HP, 1),
            ])
            .with_data(signature.iter().chain(message.iter()).copied().collect()),
    );

    run_group(
        &mut group,
        "s256",
        VmBench::new(Opcode::S256(0x10, 0x00, 0x11))
            .with_prepare_script(vec![
                Opcode::MOVI(0x10, Bytes32::LEN as Immediate18),
                Opcode::ALOC(0x10),
                Opcode::ADDI(0x10, REG_HP, 1),
                Opcode::MOVI(0x11, 32),
            ])
            .with_data(signature.iter().chain(message.iter()).copied().collect()),
    );

    run_group(
        &mut group,
        "k256",
        VmBench::new(Opcode::K256(0x10, 0x00, 0x11))
            .with_prepare_script(vec![
                Opcode::MOVI(0x10, Bytes32::LEN as Immediate18),
                Opcode::ALOC(0x10),
                Opcode::ADDI(0x10, REG_HP, 1),
                Opcode::MOVI(0x11, 32),
            ])
            .with_data(signature.iter().chain(message.iter()).copied().collect()),
    );

    group.finish();
}
