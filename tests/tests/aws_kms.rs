use fuel_core::{
    combined_database::CombinedDatabase,
    state::rocks_db::ColumnsPolicy,
};
use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::blockchain::consensus::Consensus;
use test_helpers::fuel_core_driver::FuelCoreDriver;

#[tokio::test]
async fn can_get_sealed_block_from_poa_produced_block_when_signing_with_kms() {
    use fuel_core_types::fuel_crypto::PublicKey;
    use k256::pkcs8::DecodePublicKey;

    // This test is only enabled if the environment variable is set
    let Some(kms_arn) = option_env!("FUEL_CORE_TEST_AWS_KMS_ARN") else {
        return;
    };

    // Get the public key for the KMS key
    let config = aws_config::load_from_env().await;
    let kms_client = aws_sdk_kms::Client::new(&config);
    let poa_public_der = kms_client
        .get_public_key()
        .key_id(kms_arn)
        .send()
        .await
        .expect("Unable to fetch public key from KMS")
        .public_key
        .unwrap()
        .into_inner();
    let poa_public = k256::PublicKey::from_public_key_der(&poa_public_der)
        .expect("invalid DER public key from AWS KMS");
    let poa_public = PublicKey::from(poa_public);

    // start node with the kms enabled and produce some blocks
    let num_blocks = 100;
    let args = vec![
        "--debug",
        "--poa-instant",
        "true",
        "--consensus-aws-kms",
        kms_arn,
    ];
    let driver = FuelCoreDriver::spawn(&args).await.unwrap();
    let _ = driver
        .client
        .produce_blocks(num_blocks, None)
        .await
        .unwrap();

    // stop the node and just grab the database
    let db_path = driver.kill().await;
    let db = CombinedDatabase::open(
        db_path.path(),
        1024 * 1024,
        Default::default(),
        512,
        ColumnsPolicy::Lazy,
    )
    .unwrap();

    let view = db.on_chain().latest_view().unwrap();

    // verify signatures and ensure that the block producer wont change
    let mut block_producer = None;
    for height in 1..=num_blocks {
        let sealed_block = view
            .get_sealed_block_by_height(&height.into())
            .unwrap()
            .expect("expected sealed block to be available");
        let block_id = sealed_block.entity.id();
        let signature = match sealed_block.consensus {
            Consensus::PoA(ref poa) => poa.signature,
            _ => panic!("Not expected consensus"),
        };
        signature
            .verify(&poa_public, &block_id.into_message())
            .expect("failed to verify signature");
        let this_bp = sealed_block
            .consensus
            .block_producer(&block_id)
            .expect("Block should have a block producer");
        if let Some(bp) = block_producer {
            assert_eq!(bp, this_bp, "Block producer changed");
        } else {
            block_producer = Some(this_bp);
        }
    }
}
