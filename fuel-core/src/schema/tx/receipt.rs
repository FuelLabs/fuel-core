use crate::schema::scalars::{HexString, HexString256};
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
    async fn pc(&self) -> Option<Word> {
        self.0.pc()
    }
    async fn is(&self) -> Option<Word> {
        self.0.is()
    }
    async fn to(&self) -> Option<HexString256> {
        self.0.to().copied().map(Into::into)
    }
    async fn to_address(&self) -> Option<HexString256> {
        self.0.to_address().copied().map(Into::into)
    }
    async fn amount(&self) -> Option<Word> {
        self.0.amount()
    }
    async fn color(&self) -> Option<HexString256> {
        self.0.color().copied().map(Into::into)
    }
    async fn gas(&self) -> Option<Word> {
        self.0.gas()
    }
    async fn a(&self) -> Option<Word> {
        self.0.a()
    }
    async fn b(&self) -> Option<Word> {
        self.0.b()
    }
    async fn val(&self) -> Option<Word> {
        self.0.val()
    }
    async fn ptr(&self) -> Option<Word> {
        self.0.ptr()
    }
    async fn digest(&self) -> Option<HexString256> {
        self.0.digest().copied().map(Into::into)
    }
    async fn reason(&self) -> Option<Word> {
        self.0.reason()
    }
    async fn ra(&self) -> Option<Word> {
        self.0.ra()
    }
    async fn rb(&self) -> Option<Word> {
        self.0.rb()
    }
    async fn rc(&self) -> Option<Word> {
        self.0.rc()
    }
    async fn rd(&self) -> Option<Word> {
        self.0.rd()
    }
    async fn len(&self) -> Option<Word> {
        self.0.len()
    }
    async fn receipt_type(&self) -> ReceiptType {
        self.0.into()
    }
    async fn raw_payload(&self) -> HexString {
        HexString(self.0.clone().to_bytes())
    }
    async fn status(&self) -> Option<bool> {
        self.0.status()
    }
    async fn gas_used(&self) -> Option<Word> {
        self.0.gas_used()
    }
}
