use crate::fuel_core_graphql_api::{
    indexation::coins_to_spend::NON_RETRYABLE_BYTE,
    storage::coins::CoinsToSpendIndexKey,
};
use fuel_core_storage::codec::{
    manual::Manual,
    primitive::utxo_id_to_bytes,
    Decode,
    Encode,
    Encoder,
};
use fuel_core_types::{
    fuel_tx::{
        Address,
        AssetId,
        TxId,
        UtxoId,
    },
    fuel_types::Nonce,
};
use std::borrow::Cow;

pub const AMOUNT_SIZE: usize = size_of::<u64>();
pub const UTXO_ID_SIZE: usize = size_of::<UtxoId>();
pub const RETRYABLE_FLAG_SIZE: usize = size_of::<u8>();

#[repr(u8)]
pub enum CoinType {
    Coin,
    Message,
}

pub const COIN_TYPE_SIZE: usize = size_of::<CoinType>();
pub const COIN_VARIANT_SIZE: usize =
    1 + Address::LEN + AssetId::LEN + AMOUNT_SIZE + UTXO_ID_SIZE + COIN_TYPE_SIZE;
pub const MESSAGE_VARIANT_SIZE: usize =
    1 + Address::LEN + AssetId::LEN + AMOUNT_SIZE + Nonce::LEN + COIN_TYPE_SIZE;

pub enum SerializedCoinsToSpendIndexKey {
    Coin([u8; COIN_VARIANT_SIZE]),
    Message([u8; MESSAGE_VARIANT_SIZE]),
}

impl Encoder for SerializedCoinsToSpendIndexKey {
    fn as_bytes(&self) -> Cow<[u8]> {
        match self {
            SerializedCoinsToSpendIndexKey::Coin(bytes) => Cow::Borrowed(bytes),
            SerializedCoinsToSpendIndexKey::Message(bytes) => Cow::Borrowed(bytes),
        }
    }
}

impl Encode<CoinsToSpendIndexKey> for Manual<CoinsToSpendIndexKey> {
    type Encoder<'a> = SerializedCoinsToSpendIndexKey;

    fn encode(t: &CoinsToSpendIndexKey) -> Self::Encoder<'_> {
        match t {
            CoinsToSpendIndexKey::Coin {
                owner,
                asset_id,
                amount,
                utxo_id,
            } => {
                let retryable_flag_bytes = NON_RETRYABLE_BYTE;

                // retryable_flag | address | asset_id | amount | utxo_id | coin_type
                let mut serialized_coin = [0u8; COIN_VARIANT_SIZE];
                let mut start = 0;
                let mut end = RETRYABLE_FLAG_SIZE;
                serialized_coin[start] = retryable_flag_bytes[0];
                start = end;
                end = end.saturating_add(Address::LEN);
                serialized_coin[start..end].copy_from_slice(owner.as_ref());
                start = end;
                end = end.saturating_add(AssetId::LEN);
                serialized_coin[start..end].copy_from_slice(asset_id.as_ref());
                start = end;
                end = end.saturating_add(AMOUNT_SIZE);
                serialized_coin[start..end].copy_from_slice(&amount.to_be_bytes());
                start = end;
                end = end.saturating_add(UTXO_ID_SIZE);
                serialized_coin[start..end].copy_from_slice(&utxo_id_to_bytes(utxo_id));
                start = end;
                serialized_coin[start] = CoinType::Coin as u8;

                SerializedCoinsToSpendIndexKey::Coin(serialized_coin)
            }
            CoinsToSpendIndexKey::Message {
                retryable_flag,
                owner,
                asset_id,
                amount,
                nonce,
            } => {
                // retryable_flag | address | asset_id | amount | nonce | coin_type
                let mut serialized_coin = [0u8; MESSAGE_VARIANT_SIZE];
                let mut start = 0;
                let mut end = RETRYABLE_FLAG_SIZE;
                serialized_coin[start] = *retryable_flag;
                start = end;
                end = end.saturating_add(Address::LEN);
                serialized_coin[start..end].copy_from_slice(owner.as_ref());
                start = end;
                end = end.saturating_add(AssetId::LEN);
                serialized_coin[start..end].copy_from_slice(asset_id.as_ref());
                start = end;
                end = end.saturating_add(AMOUNT_SIZE);
                serialized_coin[start..end].copy_from_slice(&amount.to_be_bytes());
                start = end;
                end = end.saturating_add(Nonce::LEN);
                serialized_coin[start..end].copy_from_slice(nonce.as_ref());
                start = end;
                serialized_coin[start] = CoinType::Message as u8;

                SerializedCoinsToSpendIndexKey::Message(serialized_coin)
            }
        }
    }
}

impl Decode<CoinsToSpendIndexKey> for Manual<CoinsToSpendIndexKey> {
    fn decode(bytes: &[u8]) -> anyhow::Result<CoinsToSpendIndexKey> {
        let coin_type = match bytes.last() {
            Some(0) => CoinType::Coin,
            Some(1) => CoinType::Message,
            _ => return Err(anyhow::anyhow!("Invalid coin type {:?}", bytes.last())),
        };

        let result = match coin_type {
            CoinType::Coin => {
                let bytes: [u8; COIN_VARIANT_SIZE] = bytes.try_into()?;
                let mut start;
                let mut end = RETRYABLE_FLAG_SIZE;
                // let retryable_flag = bytes[start..end];
                start = end;
                end = end.saturating_add(Address::LEN);
                let owner = Address::try_from(&bytes[start..end])?;
                start = end;
                end = end.saturating_add(AssetId::LEN);
                let asset_id = AssetId::try_from(&bytes[start..end])?;
                start = end;
                end = end.saturating_add(AMOUNT_SIZE);
                let amount = u64::from_be_bytes(bytes[start..end].try_into()?);
                start = end;
                end = end.saturating_add(UTXO_ID_SIZE);

                let (tx_id_bytes, output_index_bytes) =
                    bytes[start..end].split_at(TxId::LEN);
                let tx_id = TxId::try_from(tx_id_bytes)?;
                let output_index = u16::from_be_bytes(output_index_bytes.try_into()?);
                let utxo_id = UtxoId::new(tx_id, output_index);

                CoinsToSpendIndexKey::Coin {
                    owner,
                    asset_id,
                    amount,
                    utxo_id,
                }
            }
            CoinType::Message => {
                let bytes: [u8; MESSAGE_VARIANT_SIZE] = bytes.try_into()?;
                let mut start = 0;
                let mut end = RETRYABLE_FLAG_SIZE;
                let retryable_flag = bytes[start..end][0];
                start = end;
                end = end.saturating_add(Address::LEN);
                let owner = Address::try_from(&bytes[start..end])?;
                start = end;
                end = end.saturating_add(AssetId::LEN);
                let asset_id = AssetId::try_from(&bytes[start..end])?;
                start = end;
                end = end.saturating_add(AMOUNT_SIZE);
                let amount = u64::from_be_bytes(bytes[start..end].try_into()?);
                start = end;
                end = end.saturating_add(Nonce::LEN);
                let nonce = Nonce::try_from(&bytes[start..end])?;

                CoinsToSpendIndexKey::Message {
                    retryable_flag,
                    owner,
                    asset_id,
                    amount,
                    nonce,
                }
            }
        };

        Ok(result)
    }
}
