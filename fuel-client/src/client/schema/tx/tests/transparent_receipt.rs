use crate::client::schema::{schema, ConversionError, ConversionError::MissingField, HexString256};
use std::convert::{TryFrom, TryInto};

#[derive(cynic::QueryFragment, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct Receipt {
    pub a: Option<i32>,
    pub b: Option<i32>,
    pub amount: Option<i32>,
    pub color: Option<HexString256>,
    pub gas: Option<i32>,
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
    pub len: Option<i32>,
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

impl TryFrom<Receipt> for fuel_vm::prelude::Receipt {
    type Error = ConversionError;

    fn try_from(schema: Receipt) -> Result<Self, Self::Error> {
        Ok(match schema.receipt_type {
            ReceiptType::Call => fuel_vm::prelude::Receipt::Call {
                id: schema.id.into(),
                to: schema
                    .to
                    .ok_or_else(|| MissingField("to".to_string()))?
                    .into(),
                amount: schema
                    .amount
                    .ok_or_else(|| MissingField("amount".to_string()))?
                    .try_into()?,
                color: schema
                    .color
                    .ok_or_else(|| MissingField("color".to_string()))?
                    .into(),
                gas: schema
                    .gas
                    .ok_or_else(|| MissingField("gas".to_string()))?
                    .try_into()?,
                a: schema
                    .a
                    .ok_or_else(|| MissingField("a".to_string()))?
                    .try_into()?,
                b: schema
                    .b
                    .ok_or_else(|| MissingField("b".to_string()))?
                    .try_into()?,
                pc: schema.pc.try_into()?,
                is: schema.is.try_into()?,
            },
            ReceiptType::Return => fuel_vm::prelude::Receipt::Return {
                id: schema.id.into(),
                val: schema
                    .val
                    .ok_or_else(|| MissingField("val".to_string()))?
                    .try_into()?,
                pc: schema.pc.try_into()?,
                is: schema.is.try_into()?,
            },
            ReceiptType::ReturnData => fuel_vm::prelude::Receipt::ReturnData {
                id: schema.id.into(),
                ptr: schema
                    .ptr
                    .ok_or_else(|| MissingField("ptr".to_string()))?
                    .try_into()?,
                len: schema
                    .len
                    .ok_or_else(|| MissingField("len".to_string()))?
                    .try_into()?,
                digest: Default::default(),
                pc: schema.pc.try_into()?,
                is: schema.is.try_into()?,
            },
            ReceiptType::Panic => fuel_vm::prelude::Receipt::Panic {
                id: schema.id.into(),
                reason: schema
                    .reason
                    .ok_or_else(|| MissingField("reason".to_string()))?
                    .try_into()?,
                pc: schema.pc.try_into()?,
                is: schema.is.try_into()?,
            },
            ReceiptType::Revert => fuel_vm::prelude::Receipt::Revert {
                id: schema.id.into(),
                ra: schema
                    .ra
                    .ok_or_else(|| MissingField("ra".to_string()))?
                    .try_into()?,
                pc: schema.pc.try_into()?,
                is: schema.is.try_into()?,
            },
            ReceiptType::Log => fuel_vm::prelude::Receipt::Log {
                id: schema.id.into(),
                ra: schema
                    .ra
                    .ok_or_else(|| MissingField("ra".to_string()))?
                    .try_into()?,
                rb: schema
                    .rb
                    .ok_or_else(|| MissingField("rb".to_string()))?
                    .try_into()?,
                rc: schema
                    .rc
                    .ok_or_else(|| MissingField("rc".to_string()))?
                    .try_into()?,
                rd: schema
                    .rd
                    .ok_or_else(|| MissingField("rd".to_string()))?
                    .try_into()?,
                pc: schema.pc.try_into()?,
                is: schema.is.try_into()?,
            },
            ReceiptType::LogData => fuel_vm::prelude::Receipt::LogData {
                id: schema.id.into(),
                ra: schema
                    .ra
                    .ok_or_else(|| MissingField("ra".to_string()))?
                    .try_into()?,
                rb: schema
                    .rb
                    .ok_or_else(|| MissingField("rb".to_string()))?
                    .try_into()?,
                ptr: schema
                    .ptr
                    .ok_or_else(|| MissingField("ptr".to_string()))?
                    .try_into()?,
                len: schema
                    .len
                    .ok_or_else(|| MissingField("len".to_string()))?
                    .try_into()?,
                digest: schema
                    .digest
                    .ok_or_else(|| MissingField("digest".to_string()))?
                    .into(),
                pc: schema.pc.try_into()?,
                is: schema.is.try_into()?,
            },
            ReceiptType::Transfer => fuel_vm::prelude::Receipt::Transfer {
                id: schema.id.into(),
                to: schema
                    .to
                    .ok_or_else(|| MissingField("to".to_string()))?
                    .into(),
                amount: schema
                    .amount
                    .ok_or_else(|| MissingField("amount".to_string()))?
                    .try_into()?,
                color: schema
                    .color
                    .ok_or_else(|| MissingField("color".to_string()))?
                    .into(),
                pc: schema.pc.try_into()?,
                is: schema.is.try_into()?,
            },
            ReceiptType::TransferOut => fuel_vm::prelude::Receipt::TransferOut {
                id: schema.id.into(),
                to: schema
                    .to
                    .ok_or_else(|| MissingField("to".to_string()))?
                    .into(),
                amount: schema
                    .amount
                    .ok_or_else(|| MissingField("amount".to_string()))?
                    .try_into()?,
                color: schema
                    .color
                    .ok_or_else(|| MissingField("color".to_string()))?
                    .into(),
                pc: schema.pc.try_into()?,
                is: schema.is.try_into()?,
            },
        })
    }
}
