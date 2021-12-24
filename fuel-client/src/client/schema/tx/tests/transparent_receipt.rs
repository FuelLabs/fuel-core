use crate::client::schema::{
    schema, ConversionError, ConversionError::MissingField, HexString, HexString256, U64,
};
use fuel_types::Word;

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct Receipt {
    pub a: Option<U64>,
    pub b: Option<U64>,
    pub amount: Option<U64>,
    pub color: Option<HexString256>,
    pub gas: Option<U64>,
    pub digest: Option<HexString256>,
    pub id: Option<HexString256>,
    pub is: Option<U64>,
    pub pc: Option<U64>,
    pub ptr: Option<U64>,
    pub ra: Option<U64>,
    pub rb: Option<U64>,
    pub rc: Option<U64>,
    pub rd: Option<U64>,
    pub reason: Option<U64>,
    pub receipt_type: ReceiptType,
    pub to: Option<HexString256>,
    pub to_address: Option<HexString256>,
    pub val: Option<U64>,
    pub len: Option<U64>,
    pub result: Option<U64>,
    pub gas_used: Option<U64>,
    pub data: Option<HexString>,
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
}

impl TryFrom<Receipt> for fuel_vm::prelude::Receipt {
    type Error = ConversionError;

    fn try_from(schema: Receipt) -> Result<Self, Self::Error> {
        Ok(match schema.receipt_type {
            ReceiptType::Call => fuel_vm::prelude::Receipt::Call {
                id: schema
                    .id
                    .ok_or_else(|| MissingField("id".to_string()))?
                    .into(),
                to: schema
                    .to
                    .ok_or_else(|| MissingField("to".to_string()))?
                    .into(),
                amount: schema
                    .amount
                    .ok_or_else(|| MissingField("amount".to_string()))?
                    .into(),
                color: schema
                    .color
                    .ok_or_else(|| MissingField("color".to_string()))?
                    .into(),
                gas: schema
                    .gas
                    .ok_or_else(|| MissingField("gas".to_string()))?
                    .into(),
                a: schema
                    .a
                    .ok_or_else(|| MissingField("a".to_string()))?
                    .into(),
                b: schema
                    .b
                    .ok_or_else(|| MissingField("b".to_string()))?
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
            ReceiptType::Return => fuel_vm::prelude::Receipt::Return {
                id: schema
                    .id
                    .ok_or_else(|| MissingField("id".to_string()))?
                    .into(),
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
            ReceiptType::ReturnData => fuel_vm::prelude::Receipt::ReturnData {
                id: schema
                    .id
                    .ok_or_else(|| MissingField("id".to_string()))?
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
                data: schema
                    .data
                    .ok_or_else(|| MissingField("data".to_string()))?
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
            ReceiptType::Panic => fuel_vm::prelude::Receipt::Panic {
                id: schema
                    .id
                    .ok_or_else(|| MissingField("id".to_string()))?
                    .into(),
                reason: schema
                    .reason
                    .ok_or_else(|| MissingField("reason".to_string()))?
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
            ReceiptType::Revert => fuel_vm::prelude::Receipt::Revert {
                id: schema
                    .id
                    .ok_or_else(|| MissingField("id".to_string()))?
                    .into(),
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
            ReceiptType::Log => fuel_vm::prelude::Receipt::Log {
                id: schema
                    .id
                    .ok_or_else(|| MissingField("id".to_string()))?
                    .into(),
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
            ReceiptType::LogData => fuel_vm::prelude::Receipt::LogData {
                id: schema
                    .id
                    .ok_or_else(|| MissingField("id".to_string()))?
                    .into(),
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
                data: schema
                    .data
                    .ok_or_else(|| MissingField("data".to_string()))?
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
            ReceiptType::Transfer => fuel_vm::prelude::Receipt::Transfer {
                id: schema
                    .id
                    .ok_or_else(|| MissingField("id".to_string()))?
                    .into(),
                to: schema
                    .to
                    .ok_or_else(|| MissingField("to".to_string()))?
                    .into(),
                amount: schema
                    .amount
                    .ok_or_else(|| MissingField("amount".to_string()))?
                    .into(),
                color: schema
                    .color
                    .ok_or_else(|| MissingField("color".to_string()))?
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
            ReceiptType::TransferOut => fuel_vm::prelude::Receipt::TransferOut {
                id: schema
                    .id
                    .ok_or_else(|| MissingField("id".to_string()))?
                    .into(),
                to: schema
                    .to
                    .ok_or_else(|| MissingField("to".to_string()))?
                    .into(),
                amount: schema
                    .amount
                    .ok_or_else(|| MissingField("amount".to_string()))?
                    .into(),
                color: schema
                    .color
                    .ok_or_else(|| MissingField("color".to_string()))?
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
            ReceiptType::ScriptResult => fuel_vm::prelude::Receipt::ScriptResult {
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
        })
    }
}
