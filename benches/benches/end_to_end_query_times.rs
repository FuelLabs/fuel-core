use fuel_core::service::{
    config::Trigger,
    Config,
    FuelService,
};
use fuel_core_chain_config::Randomize;
use fuel_core_client::client::{
    pagination::{
        PageDirection,
        PaginationRequest,
    },
    FuelClient,
};

use fuel_core_types::fuel_tx::Address;
use rand::{
    rngs::StdRng,
    SeedableRng,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Setting up bench harness.");
    let mut harness = Harness::new(StdRng::seed_from_u64(2322)).await?;

    println!("Populating storage with transactions.");
    harness.produce_blocks_with_transactions().await?;

    println!("Querying transactions from storage.");
    harness.query_transactions_multiple_times().await?;

    println!("Shutting down.");
    harness.shutdown();

    Ok(())
}

struct Harness<Rng> {
    rng: Rng,
    params: Parameters,
    client: FuelClient,
    node: FuelService,
    owner_address: Address,
}

impl<Rng: rand::RngCore + rand::CryptoRng> Harness<Rng> {
    async fn new(mut rng: Rng) -> anyhow::Result<Self> {
        let params = Parameters::hard_coded();

        let mut config = Config::local_node();
        config.block_production = Trigger::Never;
        config.graphql_config.max_queries_complexity = usize::MAX;

        let node = FuelService::new_node(config).await?;
        let client = FuelClient::from(node.bound_address);
        let owner_address = Address::randomize(&mut rng);

        Ok(Self {
            rng,
            params,
            client,
            node,
            owner_address,
        })
    }

    async fn produce_blocks_with_transactions(&mut self) -> anyhow::Result<()> {
        for _ in 0..self.params.num_blocks {
            for tx in (1..=self.params.tx_count_per_block).map(|i| {
                test_helpers::make_tx_with_recipient(
                    &mut self.rng,
                    i,
                    u64::MAX / 2,
                    self.owner_address,
                )
            }) {
                let _tx_id = self.client.submit(&tx).await?;
            }
            self.client.produce_blocks(1, None).await?;
        }

        Ok(())
    }

    async fn query_transactions_multiple_times(&self) -> anyhow::Result<()> {
        for _ in 0..self.params.num_queries {
            let request = PaginationRequest {
                cursor: None,
                results: 100_000,
                direction: PageDirection::Forward,
            };

            self.client
                .transactions_by_owner(&self.owner_address, request)
                .await?;
        }

        Ok(())
    }

    fn shutdown(self) {
        drop(self.node);
    }
}

struct Parameters {
    num_queries: usize,
    num_blocks: usize,
    tx_count_per_block: u64,
}

impl Parameters {
    fn hard_coded() -> Self {
        Self {
            num_queries: 10,
            num_blocks: 1000,
            tx_count_per_block: 100,
        }
    }
}
