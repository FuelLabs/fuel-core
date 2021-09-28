use crate::client::schema::{schema, HexString, HexString256};

#[derive(cynic::FragmentArguments, Debug)]
pub struct TxIdArgs {
    pub id: HexString256,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    argument_struct = "TxIdArgs"
)]
pub struct TransactionQuery {
    #[arguments(id = &args.id)]
    pub transaction: Option<Transaction>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct Transaction {
    pub gas_limit: i32,
    pub gas_price: i32,
    pub id: HexString256,
    pub input_colors: Vec<HexString256>,
    pub input_contracts: Vec<HexString256>,
    pub inputs: Vec<Input>,
    pub is_script: bool,
    pub outputs: Vec<Output>,
    pub maturity: i32,
    pub receipts_root: Option<HexString256>,
    pub status: Option<Status>,
    pub witnesses: Vec<HexString>,
    pub receipts: Option<Vec<Receipt>>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct Receipt {
    pub a: Option<i32>,
    pub b: Option<i32>,
    pub amount: Option<i32>,
    pub color: Option<HexString256>,
    pub digest: Option<HexString256>,
    pub id: HexString256,
    pub is: i32,
    pub pc: i32,
    pub ptr: Option<i32>,
    pub ra: Option<i32>,
    pub rb: Option<i32>,
    pub rc: Option<i32>,
    pub rd: Option<i32>,
    pub reason: Option<i32>,
    pub receipt_type: ReceiptType,
    pub to: Option<HexString256>,
    pub to_address: Option<HexString256>,
    pub val: Option<i32>,
}

#[derive(cynic::Enum, Clone, Copy, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum ReceiptType {
    Call,
    Return,
    ReturnData,
    Panic,
    Revert,
    Log,
    LogData,
    Transfer,
    TransferOut,
}

#[derive(cynic::InlineFragments, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum Input {
    InputCoin(InputCoin),
    InputContract(InputContract),
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct InputCoin {
    pub utxo_id: HexString256,
    pub owner: HexString256,
    pub amount: i32,
    pub color: HexString256,
    pub witness_index: i32,
    pub maturity: i32,
    pub predicate: HexString,
    pub predicate_data: HexString,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct InputContract {
    pub utxo_id: HexString256,
    pub balance_root: HexString256,
    pub state_root: HexString256,
    pub contract_id: HexString256,
}

#[derive(cynic::InlineFragments, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum Output {
    CoinOutput(CoinOutput),
    ContractOutput(ContractOutput),
    WithdrawalOutput(WithdrawalOutput),
    ChangeOutput(ChangeOutput),
    VariableOutput(VariableOutput),
    ContractCreated(ContractCreated),
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct CoinOutput {
    pub to: HexString256,
    pub amount: i32,
    pub color: HexString256,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct WithdrawalOutput {
    pub to: HexString256,
    pub amount: i32,
    pub color: HexString256,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ChangeOutput {
    pub to: HexString256,
    pub amount: i32,
    pub color: HexString256,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct VariableOutput {
    pub to: HexString256,
    pub amount: i32,
    pub color: HexString256,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ContractOutput {
    pub input_index: i32,
    pub balance_root: HexString256,
    pub state_root: HexString256,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ContractCreated {
    contract_id: HexString256,
}

#[derive(cynic::InlineFragments, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum Status {
    SubmittedStatus(SubmittedStatus),
    SuccessStatus(SuccessStatus),
    FailureStatus(FailureStatus),
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct SubmittedStatus {
    pub time: super::DateTime,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct SuccessStatus {
    pub block_id: HexString256,
    pub program_state: HexString,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct FailureStatus {
    pub block_id: HexString256,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transaction_by_id_query_gql_output() {
        use cynic::QueryBuilder;
        let operation = TransactionQuery::build(TxIdArgs {
            id: HexString256("".to_string()),
        });
        insta::assert_snapshot!(operation.query)
    }
}
