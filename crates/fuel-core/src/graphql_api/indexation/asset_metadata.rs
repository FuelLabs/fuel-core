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
                let new_supply = current_supply(block_st_transaction, receipt, asset_id)?;

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

fn current_supply<T>(
    block_st_transaction: &mut T,
    receipt: &Receipt,
    asset_id: fuel_core_types::fuel_tx::AssetId,
) -> Result<u128, IndexationError>
where
    T: OffChainDatabaseTransaction,
{
    let current_supply = block_st_transaction
        .storage::<AssetsInfo>()
        .get(&asset_id)?
        .map(|info| info.total_supply)
        .unwrap_or_default();
    let new_supply = match receipt {
        Receipt::Mint { val, .. } => current_supply.checked_add(*val as u128).ok_or(
            IndexationError::AssetMetadataWouldOverflow {
                asset_id,
                current_supply,
                minted_amount: *val,
            },
        )?,
        Receipt::Burn { val, .. } => current_supply.checked_sub(*val as u128).ok_or(
            IndexationError::TryingToBurnMoreThanSupply {
                current_supply,
                burned_amount: *val,
            },
        )?,
        // TODO[RC]: We can get rid of this boilerplate by updating `enum Receipt`
        // to have a method that returns the name of the receipt or decorate with with
        // `strum_macros::AsRefStr` and use `receipt.as_ref()`. But this requires
        // a PR to `fuel_vm` first.
        Receipt::Call { .. } => {
            return Err(IndexationError::UnexpectedReceipt {
                receipt: "Call".to_string(),
            })
        }
        Receipt::Return { .. } => {
            return Err(IndexationError::UnexpectedReceipt {
                receipt: "Return".to_string(),
            })
        }
        Receipt::ReturnData { .. } => {
            return Err(IndexationError::UnexpectedReceipt {
                receipt: "ReturnData".to_string(),
            })
        }
        Receipt::Panic { .. } => {
            return Err(IndexationError::UnexpectedReceipt {
                receipt: "Panic".to_string(),
            })
        }
        Receipt::Revert { .. } => {
            return Err(IndexationError::UnexpectedReceipt {
                receipt: "Revert".to_string(),
            })
        }
        Receipt::Log { .. } => {
            return Err(IndexationError::UnexpectedReceipt {
                receipt: "Log".to_string(),
            })
        }
        Receipt::LogData { .. } => {
            return Err(IndexationError::UnexpectedReceipt {
                receipt: "LogData".to_string(),
            })
        }
        Receipt::Transfer { .. } => {
            return Err(IndexationError::UnexpectedReceipt {
                receipt: "Transfer".to_string(),
            })
        }
        Receipt::TransferOut { .. } => {
            return Err(IndexationError::UnexpectedReceipt {
                receipt: "TransferOut".to_string(),
            })
        }
        Receipt::ScriptResult { .. } => {
            return Err(IndexationError::UnexpectedReceipt {
                receipt: "ScriptResult".to_string(),
            })
        }
        Receipt::MessageOut { .. } => {
            return Err(IndexationError::UnexpectedReceipt {
                receipt: "MessageOut".to_string(),
            })
        }
    };
    Ok(new_supply)
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
