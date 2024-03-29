use crate::client::{
    schema,
    types::primitives::{
        Address,
        AssetId,
        Nonce,
        UtxoId,
    },
    PaginatedResult,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoinType {
    Coin(Coin),
    MessageCoin(MessageCoin),
    Unknown,
}

impl CoinType {
    pub fn amount(&self) -> u64 {
        match self {
            CoinType::Coin(c) => c.amount,
            CoinType::MessageCoin(m) => m.amount,
            CoinType::Unknown => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Coin {
    pub amount: u64,
    pub block_created: u32,
    pub tx_created_idx: u16,
    pub asset_id: AssetId,
    pub utxo_id: UtxoId,
    pub owner: Address,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MessageCoin {
    pub amount: u64,
    pub sender: Address,
    pub recipient: Address,
    pub nonce: Nonce,
    pub da_height: u64,
}

// GraphQL Translation

impl From<schema::coins::CoinType> for CoinType {
    fn from(value: schema::coins::CoinType) -> Self {
        match value {
            schema::coins::CoinType::Coin(coin) => Self::Coin(coin.into()),
            schema::coins::CoinType::MessageCoin(message_coin) => {
                Self::MessageCoin(message_coin.into())
            }
            schema::coins::CoinType::Unknown => Self::Unknown,
        }
    }
}

impl From<schema::coins::Coin> for Coin {
    fn from(value: schema::coins::Coin) -> Self {
        Self {
            amount: value.amount.into(),
            block_created: value.block_created.into(),
            tx_created_idx: value.tx_created_idx.into(),
            asset_id: value.asset_id.into(),
            utxo_id: value.utxo_id.into(),
            owner: value.owner.into(),
        }
    }
}

impl From<schema::coins::MessageCoin> for MessageCoin {
    fn from(value: schema::coins::MessageCoin) -> Self {
        Self {
            amount: value.amount.into(),
            sender: value.sender.into(),
            recipient: value.recipient.into(),
            nonce: value.nonce.into(),
            da_height: value.da_height.into(),
        }
    }
}

impl From<schema::coins::CoinConnection> for PaginatedResult<Coin, String> {
    fn from(conn: schema::coins::CoinConnection) -> Self {
        PaginatedResult {
            cursor: conn.page_info.end_cursor,
            has_next_page: conn.page_info.has_next_page,
            has_previous_page: conn.page_info.has_previous_page,
            results: conn.edges.into_iter().map(|e| e.node.into()).collect(),
        }
    }
}
