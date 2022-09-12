use crate::client::schema::{
    coin::Coin,
    message::Message,
    schema,
    Address,
    AssetId,
    ConversionError,
    MessageId,
    UtxoId,
    U64,
};
use itertools::Itertools;
use std::str::FromStr;

#[derive(cynic::InputObject, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ExcludeInput {
    /// Utxos to exclude from the result.
    utxos: Vec<UtxoId>,
    /// Messages to exclude from teh result.
    messages: Vec<MessageId>,
}

impl ExcludeInput {
    pub fn from_tuple(tuple: (Vec<&str>, Vec<&str>)) -> Result<Self, ConversionError> {
        let utxos = tuple.0.into_iter().map(UtxoId::from_str).try_collect()?;
        let messages = tuple.1.into_iter().map(MessageId::from_str).try_collect()?;

        Ok(Self { utxos, messages })
    }
}

#[derive(cynic::InputObject, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct SpendQueryElementInput {
    /// asset ID of the coins
    pub asset_id: AssetId,
    /// address of the owner
    pub amount: U64,
}

#[derive(cynic::InlineFragments, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum Banknote {
    Coin(Coin),
    Message(Message),
}

#[derive(cynic::FragmentArguments, Debug)]
pub struct BanknotesToSpendArgs {
    /// The Address of the utxo owner
    owner: Address,
    /// The total amount of each asset type to spend
    assets: Vec<SpendQueryElementInput>,
    /// The max number of utxos that can be used
    max_inputs: Option<i32>,
    /// A list of ids to exclude from the selection
    excluded_ids: Option<ExcludeInput>,
}

pub(crate) type BanknotesToSpendArgsTuple = (
    Address,
    Vec<SpendQueryElementInput>,
    Option<i32>,
    Option<ExcludeInput>,
);

impl From<BanknotesToSpendArgsTuple> for BanknotesToSpendArgs {
    fn from(r: BanknotesToSpendArgsTuple) -> Self {
        BanknotesToSpendArgs {
            owner: r.0,
            assets: r.1,
            max_inputs: r.2,
            excluded_ids: r.3,
        }
    }
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "BanknotesToSpendArgs"
)]
pub struct BanknotesToSpendQuery {
    #[arguments(owner = &args.owner, assets = &args.assets, max_inputs = &args.max_inputs, excluded_ids = &args.excluded_ids)]
    pub banknotes_to_spend: Vec<Vec<Banknote>>,
}
