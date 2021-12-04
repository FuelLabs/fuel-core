use crate::schema::scalars::{HexString, HexString256, U64};
use async_graphql::{Enum, Object};
use derive_more::Display;
use fuel_asm::Word;
use fuel_tx::Receipt as TxReceipt;
use fuel_types::bytes::SerializableVec;

#[derive(Copy, Clone, Debug, Display, Enum, Eq, PartialEq)]
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

impl From<TxReceipt> for ReceiptType {
    fn from(r: TxReceipt) -> Self {
        match r {
            TxReceipt::Call { .. } => ReceiptType::Call,
            TxReceipt::Return { .. } => ReceiptType::Return,
            TxReceipt::ReturnData { .. } => ReceiptType::ReturnData,
            TxReceipt::Panic { .. } => ReceiptType::Panic,
            TxReceipt::Revert { .. } => ReceiptType::Revert,
            TxReceipt::Log { .. } => ReceiptType::Log,
            TxReceipt::LogData { .. } => ReceiptType::LogData,
            TxReceipt::Transfer { .. } => ReceiptType::Transfer,
            TxReceipt::TransferOut { .. } => ReceiptType::TransferOut,
            TxReceipt::ScriptResult { .. } => ReceiptType::ScriptResult,
        }
    }
}

pub struct Receipt(pub fuel_tx::Receipt);

#[Object]
impl Receipt {
    async fn id(&self) -> Option<HexString256> {
        Some((*self.0.id()?).into())
    }
    async fn pc(&self) -> Option<U64> {
        self.0.pc().map(Into::into)
    }
    async fn is(&self) -> Option<U64> {
        self.0.is().map(Into::into)
    }
    async fn to(&self) -> Option<HexString256> {
        self.0.to().copied().map(Into::into)
    }
    async fn to_address(&self) -> Option<HexString256> {
        self.0.to_address().copied().map(Into::into)
    }
    async fn amount(&self) -> Option<U64> {
        self.0.amount().map(Into::into)
    }
    async fn color(&self) -> Option<HexString256> {
        self.0.color().copied().map(Into::into)
    }
    async fn gas(&self) -> Option<U64> {
        self.0.gas().map(Into::into)
    }
    async fn a(&self) -> Option<U64> {
        self.0.a().map(Into::into)
    }
    async fn b(&self) -> Option<U64> {
        self.0.b().map(Into::into)
    }
    async fn val(&self) -> Option<U64> {
        self.0.val().map(Into::into)
    }
    async fn ptr(&self) -> Option<U64> {
        self.0.ptr().map(Into::into)
    }
    async fn digest(&self) -> Option<HexString256> {
        self.0.digest().copied().map(Into::into)
    }
    async fn reason(&self) -> Option<U64> {
        self.0.reason().map(Into::into)
    }
    async fn ra(&self) -> Option<U64> {
        self.0.ra().map(Into::into)
    }
    async fn rb(&self) -> Option<U64> {
        self.0.rb().map(Into::into)
    }
    async fn rc(&self) -> Option<U64> {
        self.0.rc().map(Into::into)
    }
    async fn rd(&self) -> Option<U64> {
        self.0.rd().map(Into::into)
    }
    async fn len(&self) -> Option<U64> {
        self.0.len().map(Into::into)
    }
    async fn receipt_type(&self) -> ReceiptType {
        self.0.into()
    }
    async fn raw_payload(&self) -> HexString {
        HexString(self.0.clone().to_bytes())
    }
    async fn result(&self) -> Option<U64> {
        self.0.result().map(|r| Word::from(*r).into())
    }
    async fn gas_used(&self) -> Option<U64> {
        self.0.gas_used().map(Into::into)
    }
}
