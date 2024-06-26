use clap::Parser;
use fuel_core::service::{
    config::fuel_core_importer::ports::Validator,
    ServiceTrait,
};
use fuel_core_storage::transactional::AtomicView;
use rand::{
    prelude::StdRng,
    Rng,
    SeedableRng,
};

#[tokio::test(flavor = "multi_thread")]
async fn execute_any_block_testnet() -> anyhow::Result<()> {
    let mut rng = StdRng::seed_from_u64(1234);

    let node = fuel_core_bin::cli::run::get_service(
        fuel_core_bin::cli::run::Command::parse_from([
            "_IGNORED_",
            "--debug",
            "--utxo-validation",
            "--poa-instant",
            "false",
            "--db-path",
            "/Users/green/.fuel-testnet-db-archive/",
            "--snapshot",
            "/Users/green/fuel/chain-configuration/ignition",
            "--port",
            "0",
        ]),
    )?;

    node.start_and_await().await?;

    let last_block_height: u32 = node
        .shared
        .database
        .on_chain()
        .latest_height()
        .unwrap()
        .unwrap()
        .into();

    for i in 0..10000 {
        let height_to_execute = rng.gen_range(1..last_block_height);

        let block = node
            .shared
            .database
            .on_chain()
            .latest_view()
            .unwrap()
            .get_sealed_block_by_height(&height_to_execute.into())
            .unwrap()
            .unwrap();
        let block = block.entity;

        println!("Validating block {i} at height {}", height_to_execute);
        let result = node.shared.executor.validate(&block).map(|_| ());
        assert_eq!(result, Ok(()))
    }

    Ok(())
}
