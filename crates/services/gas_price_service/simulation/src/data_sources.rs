use async_trait::async_trait;
use fuel_core_gas_price_service::{
    common::{
        l2_block_source::L2BlockSource,
        utils::BlockInfo,
    },
    v1::{
        algorithm::SharedV1Algorithm,
        da_source_service::{
            service::DaBlockCostsSource,
            DaBlockCosts,
        },
    },
};

use fuel_core_gas_price_service::common::utils::{
    Error as GasPriceError,
    Result as GasPriceResult,
};
use fuel_core_types::fuel_types::BlockHeight;

pub struct SimulatedL2Blocks {
    recv: tokio::sync::mpsc::Receiver<BlockInfo>,
    shared_algo: SharedV1Algorithm,
}

impl SimulatedL2Blocks {
    pub fn new(
        recv: tokio::sync::mpsc::Receiver<BlockInfo>,
        shared_algo: SharedV1Algorithm,
    ) -> Self {
        Self { recv, shared_algo }
    }

    pub fn new_with_sender(
        shared_algo: SharedV1Algorithm,
    ) -> (Self, tokio::sync::mpsc::Sender<BlockInfo>) {
        let (send, recv) = tokio::sync::mpsc::channel(16);
        (Self { recv, shared_algo }, send)
    }
}

#[async_trait]
impl L2BlockSource for SimulatedL2Blocks {
    async fn get_l2_block(&mut self) -> GasPriceResult<BlockInfo> {
        let block = self.recv.recv().await.ok_or({
            GasPriceError::CouldNotFetchL2Block {
                source_error: anyhow::anyhow!("no more blocks; channel closed"),
            }
        })?;

        let BlockInfo::Block {
            gas_used,
            height,
            block_gas_capacity,
            block_bytes,
            ..
        } = block
        else {
            return Err(GasPriceError::CouldNotFetchL2Block {
                source_error: anyhow::anyhow!("unexpected genesis block"),
            });
        };

        let gas = gas_used;
        let new_gas_price = self.shared_algo.next_gas_price();
        let gas_price_factor = 1_150_000; // TODO: Read from CLI/config

        let mut fee = (gas as u128).checked_mul(new_gas_price as u128).expect(
            "Impossible to overflow because multiplication of two `u64` <= `u128`",
        );
        fee = fee.div_ceil(gas_price_factor as u128);

        let block = BlockInfo::Block {
            height,
            gas_used,
            block_gas_capacity,
            block_bytes,
            block_fees: fee.try_into().expect("overflow"),
            gas_price: new_gas_price,
        };
        Ok(block)
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
