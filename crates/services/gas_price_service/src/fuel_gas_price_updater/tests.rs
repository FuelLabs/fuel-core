#![allow(non_snake_case)]

use super::*;
use std::sync::Arc;
use tokio::sync::{
    mpsc::Receiver,
    Mutex,
};

struct FakeL2BlockSource {
    l2_block: Arc<Mutex<Receiver<BlockInfo>>>,
}

#[async_trait::async_trait]
impl L2BlockSource for FakeL2BlockSource {
    async fn get_l2_block(&self, _height: BlockHeight) -> Result<BlockInfo> {
        let block = self.l2_block.lock().await.recv().await.unwrap();
        Ok(block)
    }
}

struct FakeDARecordSource;

#[async_trait::async_trait]
impl DARecordSource for FakeDARecordSource {
    async fn get_da_record(&self) -> Result<Vec<RecordedBlock>> {
        Ok(vec![])
    }
}

fn arb_inner_updater() -> AlgorithmUpdaterV1 {
    AlgorithmUpdaterV1 {
        exec_gas_price_change_percent: 10,
        //
        min_gas_price: 10,
        l2_block_height: 0,
        l2_block_fullness_threshold_percent: 0,
        max_da_gas_price_change_percent: 0,
        total_da_rewards: 0,
        da_recorded_block_height: 0,
        latest_known_total_da_cost: 0,
        projected_total_da_cost: 0,
        da_p_component: 0,
        da_d_component: 0,
        profit_avg: 0,
        avg_window: 0,
        latest_da_cost_per_byte: 0,
        new_exec_price: 100,
        last_da_gas_price: 0,
        unrecorded_blocks: vec![],
    }
}

#[tokio::test]
async fn next__fetches_l2_block() {
    // given
    let l2_block = BlockInfo {
        height: 1,
        fullness: (60, 100),
        block_bytes: 1000,
        gas_price: 200,
    };
    let (l2_block_sender, l2_block_receiver) = tokio::sync::mpsc::channel(1);
    let l2_block_source = FakeL2BlockSource {
        l2_block: Arc::new(Mutex::new(l2_block_receiver)),
    };

    let da_record_source = FakeDARecordSource;
    let inner = arb_inner_updater();
    let mut updater = FuelGasPriceUpdater::new(inner, l2_block_source, da_record_source);

    let start = updater.start(0.into());
    // when
    let next = tokio::spawn(async move { updater.next().await });

    l2_block_sender.send(l2_block).await.unwrap();
    let new = next.await.unwrap().unwrap();

    // then
    dbg!(&start, &new);
    assert_ne!(start, new);
}
