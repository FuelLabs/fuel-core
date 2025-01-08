use fuel_core_types::fuel_tx::{
    ContractIdExt,
    Receipt,
};

use fuel_core_storage::StorageAsMut;

use crate::graphql_api::{
    ports::worker::OffChainDatabaseTransaction,
    storage::assets::{
        AssetDetails,
        AssetsInfo,
    },
};

use super::error::IndexationError;

pub(crate) fn update<T>(
    receipts: &[Receipt],
    block_st_transaction: &mut T,
    enabled: bool,
) -> Result<(), IndexationError>
where
    T: OffChainDatabaseTransaction,
{
    if !enabled {
        return Ok(());
    }

    for receipt in receipts {
        match receipt {
            Receipt::Mint {
                sub_id,
                contract_id,
                ..
            }
            | Receipt::Burn {
                sub_id,
                contract_id,
                ..
            } => {
                let asset_id = contract_id.asset_id(sub_id);
                let current_supply = block_st_transaction
                    .storage::<AssetsInfo>()
                    .get(&asset_id)?
                    .map(|info| info.total_supply)
                    .unwrap_or_default();

                let new_supply = match receipt {
                    Receipt::Mint { val, .. } => current_supply
                        .checked_add(*val as u128)
                        .expect("Impossible to overflow, because `val` is `u64`"),
                    Receipt::Burn { val, .. } => current_supply
                        .checked_sub(*val as u128)
                        .ok_or(IndexationError::TryingToBurnMoreThanSupply {
                            current_supply,
                            burned_amount: *val,
                        })?,
                    _ => unreachable!(),
                };

                block_st_transaction.storage::<AssetsInfo>().insert(
                    &asset_id,
                    &AssetDetails {
                        contract_id: *contract_id,
                        sub_id: *sub_id,
                        total_supply: new_supply,
                    },
                )?;
            }
            Receipt::Call { .. }
            | Receipt::Return { .. }
            | Receipt::ReturnData { .. }
            | Receipt::Panic { .. }
            | Receipt::Revert { .. }
            | Receipt::Log { .. }
            | Receipt::LogData { .. }
            | Receipt::Transfer { .. }
            | Receipt::TransferOut { .. }
            | Receipt::ScriptResult { .. }
            | Receipt::MessageOut { .. } => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn asset_metadata_index_is_correctly_updated() {
        // TODO[RC]:
    }

    #[test]
    fn asset_metadata_indexation_enabled_flag_is_respected() {
        // TODO[RC]:
    }
}
