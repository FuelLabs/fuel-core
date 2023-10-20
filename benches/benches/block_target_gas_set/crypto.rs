use crate::{
    utils::generate_linear_costs,
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

    let message = fuel_core_types::fuel_crypto::Message::new(b"foo");
    let ed19_keypair =
        ed25519_dalek::Keypair::generate(&mut ed25519_dalek_old_rand::rngs::OsRng {});
    let ed19_signature = ed19_keypair.sign(&*message);

    run(
        "crypto/ed19 opcode",
        group,
        [
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
            op::addi(0x22, 0x21, message.as_ref().len().try_into().unwrap()),
            op::movi(0x10, ed25519_dalek::PUBLIC_KEY_LENGTH.try_into().unwrap()),
            op::aloc(0x10),
            op::move_(0x11, RegId::HP),
            op::ed19(0x20, 0x21, 0x22),
            op::jmpb(RegId::ZERO, 0),
        ]
        .to_vec(),
        ed19_keypair
            .public
            .as_ref()
            .iter()
            .chain(ed19_signature.as_ref())
            .chain(message.as_ref())
            .copied()
            .collect(),
    );

    for i in generate_linear_costs() {
        let id = format!("crypto/s256 opcode {:?}", i);
        run(
            &id,
            group,
            [
                op::movi(0x11, 32),
                op::aloc(0x11),
                op::movi(0x10, i),
                op::s256(RegId::HP, RegId::ZERO, 0x10),
                op::jmpb(RegId::ZERO, 0),
            ]
            .to_vec(),
            vec![],
        )
    }

    for i in generate_linear_costs() {
        let id = format!("crypto/k256 opcode {:?}", i);
        run(
            &id,
            group,
            [
                op::movi(0x11, 32),
                op::aloc(0x11),
                op::movi(0x10, i),
                op::k256(RegId::HP, RegId::ZERO, 0x10),
                op::jmpb(RegId::ZERO, 0),
            ]
            .to_vec(),
            vec![],
        )
    }
}
