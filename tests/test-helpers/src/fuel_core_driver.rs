use clap::Parser;
use fuel_core::service::FuelService;
use fuel_core_client::client::FuelClient;
use tempfile::{
    tempdir,
    TempDir,
};

pub struct FuelCoreDriver {
    /// This must be before the db_dir as the drop order matters here
    pub node: FuelService,
    pub db_dir: TempDir,
    pub client: FuelClient,
}
impl FuelCoreDriver {
    pub async fn spawn(extra_args: &[&str]) -> anyhow::Result<Self> {
        Self::spawn_with_directory(tempdir()?, extra_args).await
    }

    pub async fn spawn_feeless(extra_args: &[&str]) -> anyhow::Result<Self> {
        let mut args = vec![
            "--starting-gas-price",
            "0",
            "--gas-price-change-percent",
            "0",
        ];
        args.extend(extra_args);
        Self::spawn_with_directory(tempdir()?, &args).await
    }

    pub async fn spawn_feeless_with_directory(
        db_dir: TempDir,
        extra_args: &[&str],
    ) -> anyhow::Result<Self> {
        let mut args = vec![
            "--starting-gas-price",
            "0",
            "--gas-price-change-percent",
            "0",
            "--min-gas-price",
            "0",
        ];
        args.extend(extra_args);
        Self::spawn_with_directory(db_dir, &args).await
    }

    pub async fn spawn_with_directory(
        db_dir: TempDir,
        extra_args: &[&str],
    ) -> anyhow::Result<Self> {
        let mut args = vec![
            "_IGNORED_",
            "--db-path",
            db_dir.path().to_str().unwrap(),
            "--port",
            "0",
        ];
        args.extend(extra_args);

        let node = fuel_core_bin::cli::run::get_service(
            fuel_core_bin::cli::run::Command::parse_from(args),
        )
        .await?;

        node.start_and_await().await?;

        let client = FuelClient::from(node.shared.graph_ql.bound_address);
        Ok(Self {
            node,
            db_dir,
            client,
        })
    }

    /// Stops the node, returning the db only
    /// Ignoring the return value drops the db as well.
    pub async fn kill(self) -> TempDir {
        println!("Stopping fuel service");
        self.node
            .send_stop_signal_and_await_shutdown()
            .await
            .expect("Failed to stop the node");
        self.db_dir
    }
}
