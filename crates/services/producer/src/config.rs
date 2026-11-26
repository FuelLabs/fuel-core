use fuel_core_types::{
    blockchain::header::{
        FEE_ASSET_STATE_TRANSITION_VERSION,
        StateTransitionBytecodeVersion,
    },
    fuel_types::{
        AssetId,
        ContractId,
    },
    services::block_producer::FeeAssetGasPrice,
};
use std::{
    collections::BTreeMap,
    num::NonZeroU64,
};

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub coinbase_recipient: Option<ContractId>,
    pub metrics: bool,
    /// Static rate in fee asset atomic units per base asset atomic unit.
    pub fee_asset_rates: BTreeMap<AssetId, NonZeroU64>,
}

impl Config {
    pub(crate) fn fee_asset_gas_prices(
        &self,
        stf: StateTransitionBytecodeVersion,
        base_gas_price: u64,
    ) -> BTreeMap<AssetId, FeeAssetGasPrice> {
        if stf < FEE_ASSET_STATE_TRANSITION_VERSION {
            return BTreeMap::new();
        }
        self.fee_asset_rates
            .iter()
            .filter_map(|(asset_id, rate)| {
                Some((
                    *asset_id,
                    FeeAssetGasPrice {
                        gas_price: base_gas_price.checked_mul(rate.get())?,
                        announce: false,
                    },
                ))
            })
            .collect()
    }
}
