use crate::client::schema;
use fuel_core_types::{
    fuel_tx::Bytes32,
    fuel_types::ContractId,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AssetDetail {
    pub contract_id: ContractId,
    pub sub_id: Bytes32,
    pub total_supply: u128,
}

// GraphQL Translation

impl From<schema::assets::AssetInfoDetails> for AssetDetail {
    fn from(value: schema::assets::AssetInfoDetails) -> Self {
        AssetDetail {
            contract_id: value.contract_id.into(),
            sub_id: value.sub_id.into(),
            total_supply: value.total_supply.into(),
        }
    }
}
