use fuel_core::{
    combined_database::CombinedDatabase,
    fuel_core_graphql_api::database::ReadDatabase,
};

use fuel_core_chain_config::Randomize;
use fuel_core_storage::{
    tables::{
        Coins,
        FuelBlocks,
    },
    transactional::WriteTransaction,
    StorageAsMut,
};
use fuel_core_types::{
    blockchain::block::CompressedBlock,
    entities::coins::coin::CompressedCoin,
    fuel_tx::{
        Address,
        AssetId,
        UtxoId,
    },
    fuel_types::BlockHeight,
};
use rand::{
    rngs::StdRng,
    seq::SliceRandom,
    SeedableRng,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Setting up bench harness.");
    let mut harness = Harness::new(StdRng::seed_from_u64(2322)).await?;

    println!("Populating storage with transactions.");
    let utxo_ids = harness.populate_database().await?;

    println!("Querying transactions from storage.");
    let total = harness
        .query_database_many_times_concurrent(&utxo_ids)
        .await?;

    println!("Queried a total of {total} coins");

    Ok(())
}

struct Harness<Rng> {
    rng: Rng,
    params: Parameters,
    db: CombinedDatabase,
}

impl<Rng: rand::RngCore + rand::CryptoRng + Send> Harness<Rng> {
    async fn new(rng: Rng) -> anyhow::Result<Self> {
        let params = Parameters::hard_coded();
        let db = CombinedDatabase::default();

        Ok(Self { rng, params, db })
    }

    async fn populate_database(&mut self) -> anyhow::Result<Vec<UtxoId>> {
        let mut utxo_ids = Vec::new();

        let mut coins: Vec<_> = (0..(self.params.utxo_count_per_block
            * self.params.num_blocks))
            .map(|_| self.generate_random_coin())
            .collect();

        for block_height in 0..self.params.num_blocks {
            let block_height = BlockHeight::from(block_height as u32);
            let mut compressed_block = CompressedBlock::default();
            compressed_block.header_mut().set_block_height(block_height);

            let mut transaction = self.db.on_chain_mut().write_transaction();

            transaction
                .storage::<FuelBlocks>()
                .insert(&block_height, &compressed_block)
                .unwrap();

            for _ in 0..self.params.utxo_count_per_block {
                let (utxo_id, coin) = coins.pop().unwrap(); // TODO: Cleanup

                transaction
                    .storage::<Coins>()
                    .insert(&utxo_id, &coin)
                    .unwrap();

                utxo_ids.push(utxo_id);
            }

            transaction.commit().unwrap();
        }

        Ok(utxo_ids)
    }

    async fn query_database_many_times_concurrent(
        &mut self,
        utxo_ids: &[UtxoId],
    ) -> anyhow::Result<usize> {
        let mut total = 0;
        let mut handles = Vec::new();
        for _ in 0..self.params.num_queries {
            let on_chain_db = self.db.on_chain().clone();
            let off_chain_db = self.db.off_chain().clone();

            let utxo_ids = utxo_ids
                .choose_multiple(&mut self.rng, self.params.num_utxos_to_query)
                .cloned()
                .collect();

            let handle = tokio::spawn(async move {
                let read_database =
                    ReadDatabase::new(0, BlockHeight::new(0), on_chain_db, off_chain_db);

                let read_view = read_database.view().unwrap();
                let res: Vec<_> = read_view.coins(utxo_ids).await.collect();
                res.len()
            });

            handles.push(handle);
        }

        for handle in handles {
            let res = handle.await.unwrap();
            total += res;
        }

        Ok(total)
    }

    fn generate_random_coin(&mut self) -> (UtxoId, CompressedCoin) {
        let utxo_id = UtxoId::randomize(&mut self.rng);

        let mut coin = CompressedCoin::default();
        coin.set_amount(self.rng.next_u64());
        coin.set_asset_id(AssetId::randomize(&mut self.rng));
        coin.set_owner(Address::randomize(&mut self.rng));

        (utxo_id, coin)
    }
}

struct Parameters {
    num_queries: usize,
    num_utxos_to_query: usize,
    num_blocks: usize,
    utxo_count_per_block: usize,
}

impl Parameters {
    fn hard_coded() -> Self {
        Self {
            num_queries: 1000,
            num_utxos_to_query: 10_000,
            num_blocks: 1000,
            utxo_count_per_block: 10_000,
        }
    }
}
