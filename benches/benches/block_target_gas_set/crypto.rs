use crate::{
    utils::arb_dependent_cost_values,
    *,
};
use rand::{
    rngs::StdRng,
    SeedableRng,
};

// ECK1: Secp251k1 signature recovery
// ECR1: Secp256r1 signature recovery
// ED19: edDSA curve25519 verification
// K256: keccak-256
// S256: SHA-2-256
pub fn run_crypto(group: &mut BenchmarkGroup<WallTime>) {
    let rng = &mut StdRng::seed_from_u64(2322u64);

    let message = Message::new(b"foo");

    let eck1_secret = SecretKey::random(rng);
    let eck1_signature = Signature::sign(&eck1_secret, &message);
    run(
        "crypto/eck1 opcode valid",
        group,
        [
            op::gtf_args(0x20, 0x00, GTFArgs::ScriptData),
            op::addi(
                0x21,
                0x20,
                eck1_signature.as_ref().len().try_into().unwrap(),
            ),
            op::movi(0x10, PublicKey::LEN.try_into().unwrap()),
            op::aloc(0x10),
            op::eck1(RegId::HP, 0x20, 0x21),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        eck1_signature
            .iter()
            .chain(message.iter())
            .copied()
            .collect(),
    );

    let wrong_message = Message::new(b"bar");

    run(
        "crypto/eck1 opcode invalid",
        group,
        [
            op::gtf_args(0x20, 0x00, GTFArgs::ScriptData),
            op::addi(
                0x21,
                0x20,
                eck1_signature.as_ref().len().try_into().unwrap(),
            ),
            op::movi(0x10, PublicKey::LEN.try_into().unwrap()),
            op::aloc(0x10),
            op::eck1(RegId::HP, 0x20, 0x21),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        eck1_signature
            .iter()
            .chain(wrong_message.iter())
            .copied()
            .collect(),
    );

    let message = fuel_core_types::fuel_crypto::Message::new(b"foo");
    let ecr1_secret = p256::ecdsa::SigningKey::random(&mut rand::thread_rng());
    let ecr1_signature = secp256r1::sign_prehashed(&ecr1_secret, &message)
        .expect("Failed to sign with secp256r1");

    run(
        "crypto/ecr1 opcode",
        group,
        [
            op::gtf_args(0x20, 0x00, GTFArgs::ScriptData),
            op::addi(
                0x21,
                0x20,
                ecr1_signature.as_ref().len().try_into().unwrap(),
            ),
            op::movi(0x10, PublicKey::LEN.try_into().unwrap()),
            op::aloc(0x10),
            op::move_(0x11, RegId::HP),
            op::ecr1(0x11, 0x20, 0x21),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        ecr1_signature
            .as_ref()
            .iter()
            .chain(message.as_ref())
            .copied()
            .collect(),
    );

    let ed19_secret = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng {});
    let ed19_signature = ed19_secret.sign(&*message);

    for i in arb_dependent_cost_values() {
        let id = format!("crypto/ed19 opcode {:?}", i);
        let message = vec![1u8; i as usize];
        run(
            &id,
            group,
            vec![
                op::gtf_args(0x20, 0x00, GTFArgs::ScriptData),
                op::addi(
                    0x21,
                    0x20,
                    ed19_secret
                        .verifying_key()
                        .as_ref()
                        .len()
                        .try_into()
                        .unwrap(),
                ),
                op::addi(
                    0x22,
                    0x21,
                    ed19_signature.to_bytes().len().try_into().unwrap(),
                ),
                op::movi(0x23, message.len().try_into().unwrap()),
                op::ed19(0x20, 0x21, 0x22, 0x23),
                op::jmpb(RegId::ZERO, 0),
            ],
            ed19_secret
                .verifying_key()
                .to_bytes()
                .iter()
                .copied()
                .chain(ed19_signature.to_bytes())
                .chain(message.iter().copied())
                .collect(),
        );
    }

    for i in arb_dependent_cost_values() {
        let id = format!("crypto/s256 opcode {:?}", i);
        run(
            &id,
            group,
            [
                op::movi(0x11, 32),
                op::aloc(0x11),
                op::movi(0x10, i),
                op::cfe(0x10),
                op::s256(RegId::HP, RegId::ZERO, 0x10),
                op::jmpb(RegId::ZERO, 0),
            ]
            .to_vec(),
            vec![],
        )
    }

    for i in arb_dependent_cost_values() {
        let id = format!("crypto/k256 opcode {:?}", i);
        run(
            &id,
            group,
            [
                op::movi(0x11, 32),
                op::aloc(0x11),
                op::movi(0x10, i),
                op::cfe(0x10),
                op::k256(RegId::HP, RegId::ZERO, 0x10),
                op::jmpb(RegId::ZERO, 0),
            ]
            .to_vec(),
            vec![],
        )
    }
}
