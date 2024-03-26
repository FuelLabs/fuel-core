use crate::ext;
use fuel_core_executor::ports::RelayerPort;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    services::relayer::Event,
};

pub struct WasmRelayer;

impl RelayerPort for WasmRelayer {
    fn enabled(&self) -> bool {
        ext::relayer_enabled()
    }

    fn get_events(&self, da_block_height: &DaBlockHeight) -> anyhow::Result<Vec<Event>> {
        ext::relayer_get_events(*da_block_height)
    }
}
