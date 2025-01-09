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
    use fuel_core_storage::{
        transactional::WriteTransaction,
        StorageAsMut,
    };
    use fuel_core_types::fuel_tx::{
        Bytes32,
        ContractId,
        ContractIdExt,
        Receipt,
    };

    use crate::{
        database::{
            database_description::off_chain::OffChain,
            Database,
        },
        graphql_api::{
            indexation::asset_metadata::update,
            storage::assets::{
                AssetDetails,
                AssetsInfo,
            },
        },
        state::rocks_db::DatabaseConfig,
    };

    #[test]
    fn asset_metadata_index_is_correctly_updated() {
        use tempfile::TempDir;
        let tmp_dir = TempDir::new().unwrap();
        let mut db: Database<OffChain> = Database::open_rocksdb(
            tmp_dir.path(),
            Default::default(),
            DatabaseConfig::config_for_tests(),
        )
        .unwrap();
        let mut tx = db.write_transaction();

        const ASSET_METADATA_IS_ENABLED: bool = true;

        let sub_id: Bytes32 = Bytes32::from([1u8; 32]);
        let contract_id: ContractId = ContractId::from([2u8; 32]);
        let contract_asset_id = contract_id.asset_id(&sub_id);
        const MINT_AMOUNT: u64 = 3;
        const BURN_AMOUNT: u64 = 2;

        let receipts: Vec<Receipt> = vec![
            Receipt::mint(sub_id, contract_id, MINT_AMOUNT, 0, 0),
            Receipt::burn(sub_id, contract_id, BURN_AMOUNT, 0, 0),
        ];

        update(&receipts, &mut tx, ASSET_METADATA_IS_ENABLED)
            .expect("should process receipt");

        let metadata = &*tx
            .storage::<AssetsInfo>()
            .get(&contract_asset_id)
            .expect("should correctly query db")
            .expect("should have metadata");

        assert_eq!(
            metadata,
            &AssetDetails {
                contract_id,
                sub_id,
                total_supply: MINT_AMOUNT as u128 - BURN_AMOUNT as u128
            }
        );
    }

    #[test]
    fn asset_metadata_indexation_enabled_flag_is_respected() {
        use tempfile::TempDir;
        let tmp_dir = TempDir::new().unwrap();
        let mut db: Database<OffChain> = Database::open_rocksdb(
            tmp_dir.path(),
            Default::default(),
            DatabaseConfig::config_for_tests(),
        )
        .unwrap();
        let mut tx = db.write_transaction();

        const ASSET_METADATA_IS_DISABLED: bool = false;

        let sub_id: Bytes32 = Bytes32::from([1u8; 32]);
        let contract_id: ContractId = ContractId::from([2u8; 32]);
        let contract_asset_id = contract_id.asset_id(&sub_id);
        const MINT_AMOUNT: u64 = 3;
        const BURN_AMOUNT: u64 = 2;

        let receipts: Vec<Receipt> = vec![
            Receipt::mint(sub_id, contract_id, MINT_AMOUNT, 0, 0),
            Receipt::burn(sub_id, contract_id, BURN_AMOUNT, 0, 0),
        ];

        update(&receipts, &mut tx, ASSET_METADATA_IS_DISABLED)
            .expect("should process receipt");

        let metadata = tx
            .storage::<AssetsInfo>()
            .get(&contract_asset_id)
            .expect("should correctly query db");

        assert!(metadata.is_none());
    }
}
