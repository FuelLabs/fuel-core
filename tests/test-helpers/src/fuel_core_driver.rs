use clap::Parser;
use fuel_core::service::FuelService;
use fuel_core_client::client::FuelClient;
use fuel_core_poa::ports::BlockImporter;
use fuel_core_types::fuel_types::BlockHeight;
use std::time::Duration;
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

    /// Wait for the node to reach the block height.
    pub async fn wait_for_block_height(&self, block_height: &BlockHeight) {
        use futures::stream::StreamExt;
        let mut blocks = self.node.shared.block_importer.block_stream();
        loop {
            let last_height = self
                .node
                .shared
                .database
                .on_chain()
                .latest_height_from_metadata()
                .unwrap()
                .unwrap();

            if last_height >= *block_height {
                break;
            }

            tokio::select! {
                result = blocks.next() => {
                    result.unwrap();
                }
                _ = self.node.await_shutdown() => {
                    panic!("Got a stop signal")
                }
            }
        }
    }

    /// Wait for the node to reach the block height 10 seconds.
    pub async fn wait_for_block_height_10s(&self, block_height: &BlockHeight) {
        tokio::time::timeout(
            Duration::from_secs(10),
            self.wait_for_block_height(block_height),
        )
        .await
        .expect("Timeout waiting for block height");
    }
}
