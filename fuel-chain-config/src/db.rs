use crate::{
    CoinConfig,
    ContractConfig,
    MessageConfig,
};
use fuel_core_interfaces::{
    common::{
        fuel_asm::Word,
        fuel_storage::StorageAsRef,
        fuel_tx::{
            AssetId,
            Bytes32,
            ContractId,
        },
    },
    model::{
        BlockHeight,
        Coin,
        CoinStatus,
    },
};
use fuel_database::{
    tables::{
        ContractsInfo,
        ContractsRawCode,
    },
    Column,
    Database,
    Error,
};

// TODO: Use either `anyhow::Error` or `Error`
pub trait ChainConfigDb {
    /// Returns *all* unspent coin configs available in the database.
    fn get_coin_config(&self) -> anyhow::Result<Option<Vec<CoinConfig>>>;
    /// Returns *alive* contract configs available in the database.
    fn get_contract_config(&self) -> anyhow::Result<Option<Vec<ContractConfig>>>;
    /// Returns *all* unspent message configs available in the database.
    fn get_message_config(&self) -> Result<Option<Vec<MessageConfig>>, Error>;
    /// Returns the last available block height.
    fn get_block_height(&self) -> Result<Option<BlockHeight>, Error>;
}

/// Implement `ChainConfigDb` so that `Database` can be passed to
/// `StateConfig's` `generate_state_config()` method
impl ChainConfigDb for Database {
    fn get_coin_config(&self) -> anyhow::Result<Option<Vec<CoinConfig>>> {
        let configs = self
            .iter_all::<Vec<u8>, Coin>(Column::Coins, None, None, None)
            .filter_map(|coin| {
                // Return only unspent coins
                if let Ok(coin) = coin {
                    if coin.1.status == CoinStatus::Unspent {
                        Some(Ok(coin))
                    } else {
                        None
                    }
                } else {
                    Some(coin)
                }
            })
            .map(|raw_coin| -> Result<CoinConfig, anyhow::Error> {
                let coin = raw_coin?;

                let byte_id = Bytes32::new(coin.0[..32].try_into()?);
                let output_index = coin.0[32];

                Ok(CoinConfig {
                    tx_id: Some(byte_id),
                    output_index: Some(output_index.into()),
                    block_created: Some(coin.1.block_created),
                    maturity: Some(coin.1.maturity),
                    owner: coin.1.owner,
                    amount: coin.1.amount,
                    asset_id: coin.1.asset_id,
                })
            })
            .collect::<Result<Vec<CoinConfig>, anyhow::Error>>()?;

        Ok(Some(configs))
    }

    fn get_contract_config(&self) -> Result<Option<Vec<ContractConfig>>, anyhow::Error> {
        let configs = self
            .iter_all::<Vec<u8>, Word>(Column::ContractsRawCode, None, None, None)
            .map(|raw_contract_id| -> Result<ContractConfig, anyhow::Error> {
                let contract_id =
                    ContractId::new(raw_contract_id.unwrap().0[..32].try_into()?);

                let code: Vec<u8> = self
                    .storage::<ContractsRawCode>()
                    .get(&contract_id)?
                    .unwrap()
                    .into_owned()
                    .into();

                let (salt, _) = self
                    .storage::<ContractsInfo>()
                    .get(&contract_id)
                    .unwrap()
                    .expect("Contract does not exist")
                    .into_owned();

                let state = Some(
                    self.iter_all::<Vec<u8>, Bytes32>(
                        Column::ContractsState,
                        Some(contract_id.as_ref().to_vec()),
                        None,
                        None,
                    )
                    .map(|res| -> Result<(Bytes32, Bytes32), anyhow::Error> {
                        let safe_res = res?;

                        // We don't need to store ContractId which is the first 32 bytes of this
                        // key, as this Vec is already attached to that ContractId
                        let state_key = Bytes32::new(safe_res.0[32..].try_into()?);

                        Ok((state_key, safe_res.1))
                    })
                    .filter(|val| val.is_ok())
                    .collect::<Result<Vec<(Bytes32, Bytes32)>, anyhow::Error>>()?,
                );

                let balances = Some(
                    self.iter_all::<Vec<u8>, u64>(
                        Column::ContractsAssets,
                        Some(contract_id.as_ref().to_vec()),
                        None,
                        None,
                    )
                    .map(|res| -> Result<(AssetId, u64), anyhow::Error> {
                        let safe_res = res?;

                        let asset_id = AssetId::new(safe_res.0[32..].try_into()?);

                        Ok((asset_id, safe_res.1))
                    })
                    .filter(|val| val.is_ok())
                    .collect::<Result<Vec<(AssetId, u64)>, anyhow::Error>>()?,
                );

                Ok(ContractConfig {
                    code,
                    salt,
                    state,
                    balances,
                })
            })
            .collect::<Result<Vec<ContractConfig>, anyhow::Error>>()?;

        Ok(Some(configs))
    }

    fn get_message_config(&self) -> Result<Option<Vec<MessageConfig>>, Error> {
        let configs = self
            .all_messages(None, None)
            .filter_map(|msg| {
                // Return only unspent messages
                if let Ok(msg) = msg {
                    if msg.fuel_block_spend.is_none() {
                        Some(Ok(msg))
                    } else {
                        None
                    }
                } else {
                    Some(msg)
                }
            })
            .map(|msg| -> Result<MessageConfig, Error> {
                let msg = msg?;

                Ok(MessageConfig {
                    sender: msg.sender,
                    recipient: msg.recipient,
                    nonce: msg.nonce,
                    amount: msg.amount,
                    data: msg.data,
                    da_height: msg.da_height,
                })
            })
            .collect::<Result<Vec<MessageConfig>, Error>>()?;

        Ok(Some(configs))
    }

    fn get_block_height(&self) -> Result<Option<BlockHeight>, Error> {
        Self::get_block_height(self)
    }
}
