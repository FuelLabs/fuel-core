use crate::client::schema::{
    schema,
    Address,
    AssetId,
    Bytes32,
    ContractId,
    ConversionError,
    ConversionError::MissingField,
    HexString,
    Nonce,
    U64,
};
use fuel_core_types::{
    fuel_asm::Word,
    fuel_tx,
};

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct Receipt {
    pub param1: Option<U64>,
    pub param2: Option<U64>,
    pub amount: Option<U64>,
    pub asset_id: Option<AssetId>,
    pub gas: Option<U64>,
    pub digest: Option<Bytes32>,
    pub id: Option<ContractId>,
    pub is: Option<U64>,
    pub pc: Option<U64>,
    pub ptr: Option<U64>,
    pub ra: Option<U64>,
    pub rb: Option<U64>,
    pub rc: Option<U64>,
    pub rd: Option<U64>,
    pub reason: Option<U64>,
    pub receipt_type: ReceiptType,
    pub to: Option<ContractId>,
    pub to_address: Option<Address>,
    pub val: Option<U64>,
    pub len: Option<U64>,
    pub result: Option<U64>,
    pub gas_used: Option<U64>,
    pub data: Option<HexString>,
    pub sender: Option<Address>,
    pub recipient: Option<Address>,
    pub nonce: Option<Nonce>,
    pub contract_id: Option<ContractId>,
    pub sub_id: Option<Bytes32>,
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
    ScriptResult,
    MessageOut,
    Mint,
    Burn,
}

impl TryFrom<Receipt> for fuel_tx::Receipt {
    type Error = ConversionError;

    fn try_from(schema: Receipt) -> Result<Self, Self::Error> {
        Ok(match schema.receipt_type {
            ReceiptType::Call => fuel_tx::Receipt::Call {
                id: schema.id.map(|id| id.into()).unwrap_or_default(),
                to: schema
                    .to
                    .ok_or_else(|| MissingField("to".to_string()))?
                    .into(),
                amount: schema
                    .amount
                    .ok_or_else(|| MissingField("amount".to_string()))?
                    .into(),
                asset_id: schema
                    .asset_id
                    .ok_or_else(|| MissingField("assetId".to_string()))?
                    .into(),
                gas: schema
                    .gas
                    .ok_or_else(|| MissingField("gas".to_string()))?
                    .into(),
                param1: schema
                    .param1
                    .ok_or_else(|| MissingField("param1".to_string()))?
                    .into(),
                param2: schema
                    .param2
                    .ok_or_else(|| MissingField("param2".to_string()))?
                    .into(),
                pc: schema
                    .pc
                    .ok_or_else(|| MissingField("pc".to_string()))?
                    .into(),
                is: schema
                    .is
                    .ok_or_else(|| MissingField("is".to_string()))?
                    .into(),
            },
            ReceiptType::Return => fuel_tx::Receipt::Return {
                id: schema.id.map(|id| id.into()).unwrap_or_default(),
                val: schema
                    .val
                    .ok_or_else(|| MissingField("val".to_string()))?
                    .into(),
                pc: schema
                    .pc
                    .ok_or_else(|| MissingField("pc".to_string()))?
                    .into(),
                is: schema
                    .is
                    .ok_or_else(|| MissingField("is".to_string()))?
                    .into(),
            },
            ReceiptType::ReturnData => fuel_tx::Receipt::ReturnData {
                id: schema.id.map(|id| id.into()).unwrap_or_default(),
                ptr: schema
                    .ptr
                    .ok_or_else(|| MissingField("ptr".to_string()))?
                    .into(),
                len: schema
                    .len
                    .ok_or_else(|| MissingField("len".to_string()))?
                    .into(),
                digest: schema
                    .digest
                    .ok_or_else(|| MissingField("digest".to_string()))?
                    .into(),
                data: Some(
                    schema
                        .data
                        .ok_or_else(|| MissingField("data".to_string()))?
                        .into(),
                ),
                pc: schema
                    .pc
                    .ok_or_else(|| MissingField("pc".to_string()))?
                    .into(),
                is: schema
                    .is
                    .ok_or_else(|| MissingField("is".to_string()))?
                    .into(),
            },
            ReceiptType::Panic => fuel_tx::Receipt::Panic {
                id: schema.id.map(|id| id.into()).unwrap_or_default(),
                reason: schema
                    .reason
                    .ok_or_else(|| MissingField("reason".to_string()))?
                    .try_into()?,
                pc: schema
                    .pc
                    .ok_or_else(|| MissingField("pc".to_string()))?
                    .into(),
                is: schema
                    .is
                    .ok_or_else(|| MissingField("is".to_string()))?
                    .into(),
                contract_id: schema.contract_id.map(Into::into),
            },
            ReceiptType::Revert => fuel_tx::Receipt::Revert {
                id: schema.id.map(|id| id.into()).unwrap_or_default(),
                ra: schema
                    .ra
                    .ok_or_else(|| MissingField("ra".to_string()))?
                    .into(),
                pc: schema
                    .pc
                    .ok_or_else(|| MissingField("pc".to_string()))?
                    .into(),
                is: schema
                    .is
                    .ok_or_else(|| MissingField("is".to_string()))?
                    .into(),
            },
            ReceiptType::Log => fuel_tx::Receipt::Log {
                id: schema.id.map(|id| id.into()).unwrap_or_default(),
                ra: schema
                    .ra
                    .ok_or_else(|| MissingField("ra".to_string()))?
                    .into(),
                rb: schema
                    .rb
                    .ok_or_else(|| MissingField("rb".to_string()))?
                    .into(),
                rc: schema
                    .rc
                    .ok_or_else(|| MissingField("rc".to_string()))?
                    .into(),
                rd: schema
                    .rd
                    .ok_or_else(|| MissingField("rd".to_string()))?
                    .into(),
                pc: schema
                    .pc
                    .ok_or_else(|| MissingField("pc".to_string()))?
                    .into(),
                is: schema
                    .is
                    .ok_or_else(|| MissingField("is".to_string()))?
                    .into(),
            },
            ReceiptType::LogData => fuel_tx::Receipt::LogData {
                id: schema.id.map(|id| id.into()).unwrap_or_default(),
                ra: schema
                    .ra
                    .ok_or_else(|| MissingField("ra".to_string()))?
                    .into(),
                rb: schema
                    .rb
                    .ok_or_else(|| MissingField("rb".to_string()))?
                    .into(),
                ptr: schema
                    .ptr
                    .ok_or_else(|| MissingField("ptr".to_string()))?
                    .into(),
                len: schema
                    .len
                    .ok_or_else(|| MissingField("len".to_string()))?
                    .into(),
                digest: schema
                    .digest
                    .ok_or_else(|| MissingField("digest".to_string()))?
                    .into(),
                data: Some(
                    schema
                        .data
                        .ok_or_else(|| MissingField("data".to_string()))?
                        .into(),
                ),
                pc: schema
                    .pc
                    .ok_or_else(|| MissingField("pc".to_string()))?
                    .into(),
                is: schema
                    .is
                    .ok_or_else(|| MissingField("is".to_string()))?
                    .into(),
            },
            ReceiptType::Transfer => fuel_tx::Receipt::Transfer {
                id: schema.id.map(|id| id.into()).unwrap_or_default(),
                to: schema
                    .to
                    .ok_or_else(|| MissingField("to".to_string()))?
                    .into(),
                amount: schema
                    .amount
                    .ok_or_else(|| MissingField("amount".to_string()))?
                    .into(),
                asset_id: schema
                    .asset_id
                    .ok_or_else(|| MissingField("assetId".to_string()))?
                    .into(),
                pc: schema
                    .pc
                    .ok_or_else(|| MissingField("pc".to_string()))?
                    .into(),
                is: schema
                    .is
                    .ok_or_else(|| MissingField("is".to_string()))?
                    .into(),
            },
            ReceiptType::TransferOut => fuel_tx::Receipt::TransferOut {
                id: schema.id.map(|id| id.into()).unwrap_or_default(),
                to: schema
                    .to_address
                    .ok_or_else(|| MissingField("to_address".to_string()))?
                    .into(),
                amount: schema
                    .amount
                    .ok_or_else(|| MissingField("amount".to_string()))?
                    .into(),
                asset_id: schema
                    .asset_id
                    .ok_or_else(|| MissingField("assetId".to_string()))?
                    .into(),
                pc: schema
                    .pc
                    .ok_or_else(|| MissingField("pc".to_string()))?
                    .into(),
                is: schema
                    .is
                    .ok_or_else(|| MissingField("is".to_string()))?
                    .into(),
            },
            ReceiptType::ScriptResult => fuel_tx::Receipt::ScriptResult {
                result: Word::from(
                    schema
                        .result
                        .ok_or_else(|| MissingField("result".to_string()))?,
                )
                .into(),
                gas_used: schema
                    .gas_used
                    .ok_or_else(|| MissingField("gas_used".to_string()))?
                    .into(),
            },
            ReceiptType::MessageOut => fuel_tx::Receipt::MessageOut {
                sender: schema
                    .sender
                    .ok_or_else(|| MissingField("sender".to_string()))?
                    .into(),
                recipient: schema
                    .recipient
                    .ok_or_else(|| MissingField("recipient".to_string()))?
                    .into(),
                amount: schema
                    .amount
                    .ok_or_else(|| MissingField("amount".to_string()))?
                    .into(),
                nonce: schema
                    .nonce
                    .ok_or_else(|| MissingField("nonce".to_string()))?
                    .into(),
                len: schema
                    .len
                    .ok_or_else(|| MissingField("len".to_string()))?
                    .into(),
                digest: schema
                    .digest
                    .ok_or_else(|| MissingField("digest".to_string()))?
                    .into(),
                data: Some(
                    schema
                        .data
                        .ok_or_else(|| MissingField("data".to_string()))?
                        .into(),
                ),
            },
            ReceiptType::Mint => fuel_tx::Receipt::Mint {
                sub_id: schema
                    .sub_id
                    .ok_or_else(|| MissingField("sub_id".to_string()))?
                    .into(),
                contract_id: schema.id.map(|id| id.into()).unwrap_or_default(),
                val: schema
                    .val
                    .ok_or_else(|| MissingField("val".to_string()))?
                    .into(),
                pc: schema
                    .pc
                    .ok_or_else(|| MissingField("pc".to_string()))?
                    .into(),
                is: schema
                    .is
                    .ok_or_else(|| MissingField("is".to_string()))?
                    .into(),
            },
            ReceiptType::Burn => fuel_tx::Receipt::Burn {
                sub_id: schema
                    .sub_id
                    .ok_or_else(|| MissingField("sub_id".to_string()))?
                    .into(),
                contract_id: schema.id.map(|id| id.into()).unwrap_or_default(),
                val: schema
                    .val
                    .ok_or_else(|| MissingField("val".to_string()))?
                    .into(),
                pc: schema
                    .pc
                    .ok_or_else(|| MissingField("pc".to_string()))?
                    .into(),
                is: schema
                    .is
                    .ok_or_else(|| MissingField("is".to_string()))?
                    .into(),
            },
        })
    }
}
