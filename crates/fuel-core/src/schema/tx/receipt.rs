use crate::schema::{
    contract::Contract,
    scalars::{
        Address,
        AssetId,
        Bytes32,
        ContractId,
        HexString,
        Nonce,
        U64,
    },
};
use async_graphql::{
    Enum,
    Object,
};
use derive_more::Display;
use fuel_core_types::{
    fuel_asm::Word,
    fuel_tx,
};

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
    MessageOut,
    Mint,
    Burn,
}

impl From<&fuel_tx::Receipt> for ReceiptType {
    fn from(r: &fuel_tx::Receipt) -> Self {
        match r {
            fuel_tx::Receipt::Call { .. } => ReceiptType::Call,
            fuel_tx::Receipt::Return { .. } => ReceiptType::Return,
            fuel_tx::Receipt::ReturnData { .. } => ReceiptType::ReturnData,
            fuel_tx::Receipt::Panic { .. } => ReceiptType::Panic,
            fuel_tx::Receipt::Revert { .. } => ReceiptType::Revert,
            fuel_tx::Receipt::Log { .. } => ReceiptType::Log,
            fuel_tx::Receipt::LogData { .. } => ReceiptType::LogData,
            fuel_tx::Receipt::Transfer { .. } => ReceiptType::Transfer,
            fuel_tx::Receipt::TransferOut { .. } => ReceiptType::TransferOut,
            fuel_tx::Receipt::ScriptResult { .. } => ReceiptType::ScriptResult,
            fuel_tx::Receipt::MessageOut { .. } => ReceiptType::MessageOut,
            fuel_tx::Receipt::Mint { .. } => ReceiptType::Mint,
            fuel_tx::Receipt::Burn { .. } => ReceiptType::Burn,
        }
    }
}

pub struct Receipt(pub fuel_tx::Receipt);

#[Object]
impl Receipt {
    async fn contract(&self) -> Option<Contract> {
        Some((*self.0.id()?).into())
    }
    async fn pc(&self) -> Option<U64> {
        self.0.pc().map(Into::into)
    }
    async fn is(&self) -> Option<U64> {
        self.0.is().map(Into::into)
    }
    async fn to(&self) -> Option<Contract> {
        self.0.to().copied().map(Into::into)
    }
    async fn to_address(&self) -> Option<Address> {
        self.0.to_address().copied().map(Into::into)
    }
    async fn amount(&self) -> Option<U64> {
        self.0.amount().map(Into::into)
    }
    async fn asset_id(&self) -> Option<AssetId> {
        self.0.asset_id().copied().map(Into::into)
    }
    async fn gas(&self) -> Option<U64> {
        self.0.gas().map(Into::into)
    }
    async fn param1(&self) -> Option<U64> {
        self.0.param1().map(Into::into)
    }
    async fn param2(&self) -> Option<U64> {
        self.0.param2().map(Into::into)
    }
    async fn val(&self) -> Option<U64> {
        self.0.val().map(Into::into)
    }
    async fn ptr(&self) -> Option<U64> {
        self.0.ptr().map(Into::into)
    }
    async fn digest(&self) -> Option<Bytes32> {
        self.0.digest().copied().map(Into::into)
    }
    async fn reason(&self) -> Option<U64> {
        self.0.reason().map(|r| U64(r.into()))
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
        (&self.0).into()
    }
    async fn result(&self) -> Option<U64> {
        self.0.result().map(|r| Word::from(*r).into())
    }
    async fn gas_used(&self) -> Option<U64> {
        self.0.gas_used().map(Into::into)
    }
    async fn data(&self) -> Option<HexString> {
        self.0.data().map(|d| d.to_vec().into())
    }
    async fn sender(&self) -> Option<Address> {
        self.0.sender().copied().map(Address)
    }
    async fn recipient(&self) -> Option<Address> {
        self.0.recipient().copied().map(Address)
    }
    async fn nonce(&self) -> Option<Nonce> {
        self.0.nonce().copied().map(Nonce)
    }
    async fn contract_id(&self) -> Option<ContractId> {
        self.0.contract_id().map(|id| ContractId(*id))
    }
    async fn sub_id(&self) -> Option<Bytes32> {
        self.0.sub_id().copied().map(Into::into)
    }
}

impl From<&fuel_tx::Receipt> for Receipt {
    fn from(receipt: &fuel_tx::Receipt) -> Self {
        Receipt(receipt.clone())
    }
}

impl From<fuel_tx::Receipt> for Receipt {
    fn from(receipt: fuel_tx::Receipt) -> Self {
        Receipt(receipt)
    }
}
