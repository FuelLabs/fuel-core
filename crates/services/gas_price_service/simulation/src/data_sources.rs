use async_trait::async_trait;
use fuel_core_gas_price_service::{
    common::{
        l2_block_source::L2BlockSource,
        utils::BlockInfo,
    },
    v1::da_source_service::{
        service::DaBlockCostsSource,
        DaBlockCosts,
    },
};

use fuel_core_gas_price_service::common::utils::{
    Error as GasPriceError,
    Result as GasPriceResult,
};
use fuel_core_types::fuel_types::BlockHeight;

pub struct SimulatedL2Blocks {
    recv: tokio::sync::mpsc::Receiver<BlockInfo>,
}

impl SimulatedL2Blocks {
    pub fn new(recv: tokio::sync::mpsc::Receiver<BlockInfo>) -> Self {
        Self { recv }
    }

    pub fn new_with_sender() -> (Self, tokio::sync::mpsc::Sender<BlockInfo>) {
        let (send, recv) = tokio::sync::mpsc::channel(16);
        (Self { recv }, send)
    }
}

#[async_trait]
impl L2BlockSource for SimulatedL2Blocks {
    async fn get_l2_block(&mut self) -> GasPriceResult<BlockInfo> {
        // TODO: do we want to modify these values to somehow reflect the previously chosen gas
        //   price better? We might be able to do that by having a handle to the shared algo.

        self.recv.recv().await.ok_or({
            GasPriceError::CouldNotFetchL2Block {
                source_error: anyhow::anyhow!("no more blocks; channel closed"),
            }
        })
    }
}

pub struct SimulatedDACosts {
    recv: tokio::sync::mpsc::Receiver<DaBlockCosts>,
}

impl SimulatedDACosts {
    pub fn new(recv: tokio::sync::mpsc::Receiver<DaBlockCosts>) -> Self {
        Self { recv }
    }

    pub fn new_with_sender() -> (Self, tokio::sync::mpsc::Sender<DaBlockCosts>) {
        let (send, recv) = tokio::sync::mpsc::channel(16);
        (Self { recv }, send)
    }
}

#[async_trait]
impl DaBlockCostsSource for SimulatedDACosts {
    async fn request_da_block_costs(
        &mut self,
        _recorded_height: &Option<BlockHeight>,
    ) -> anyhow::Result<Vec<DaBlockCosts>> {
        self.recv
            .recv()
            .await
            .map(|costs| vec![costs])
            .ok_or(anyhow::anyhow!("no more costs; channel closed"))
    }
}
