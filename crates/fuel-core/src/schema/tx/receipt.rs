use crate::schema::scalars::{
    Address,
    AssetId,
    Bytes32,
    ContractId,
    HexString,
    Nonce,
    U64,
};
use async_graphql::{
    Enum,
    Object,
};
use fuel_core_types::{
    fuel_asm::Word,
    fuel_tx,
};

#[derive(
    Copy, Clone, Debug, derive_more::Display, Enum, Eq, PartialEq, strum_macros::EnumIter,
)]
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
    async fn id(&self) -> Option<ContractId> {
        Some((*self.0.id()?).into())
    }
    async fn pc(&self) -> Option<U64> {
        self.0.pc().map(Into::into)
    }
    async fn is(&self) -> Option<U64> {
        self.0.is().map(Into::into)
    }
    async fn to(&self) -> Option<ContractId> {
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

    /// Set in the case of a Panic receipt to indicate a missing contract input id
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

#[cfg(feature = "test-helpers")]
pub fn all_receipts() -> Vec<fuel_tx::Receipt> {
    use strum::IntoEnumIterator;

    let mut receipts = vec![];
    for variant in ReceiptType::iter() {
        let receipt = match variant {
            ReceiptType::Call => fuel_tx::Receipt::call(
                fuel_tx::ContractId::from([1u8; 32]),
                fuel_tx::ContractId::from([2u8; 32]),
                3,
                fuel_tx::AssetId::from([3u8; 32]),
                5,
                6,
                7,
                8,
                9,
            ),
            ReceiptType::Return => {
                fuel_tx::Receipt::ret(fuel_tx::ContractId::from([1u8; 32]), 2, 3, 4)
            }
            ReceiptType::ReturnData => fuel_tx::Receipt::return_data(
                fuel_tx::ContractId::from([1u8; 32]),
                2,
                3,
                4,
                vec![5; 30],
            ),
            ReceiptType::Panic => fuel_tx::Receipt::panic(
                fuel_tx::ContractId::from([1u8; 32]),
                fuel_core_types::fuel_asm::PanicInstruction::error(
                    fuel_core_types::fuel_asm::PanicReason::ArithmeticOverflow,
                    2,
                ),
                3,
                4,
            ),
            ReceiptType::Revert => {
                fuel_tx::Receipt::revert(fuel_tx::ContractId::from([1u8; 32]), 2, 3, 4)
            }
            ReceiptType::Log => fuel_tx::Receipt::log(
                fuel_tx::ContractId::from([1u8; 32]),
                2,
                3,
                4,
                5,
                6,
                7,
            ),
            ReceiptType::LogData => fuel_tx::Receipt::log_data(
                fuel_tx::ContractId::from([1u8; 32]),
                2,
                3,
                4,
                5,
                6,
                vec![7; 30],
            ),
            ReceiptType::Transfer => fuel_tx::Receipt::transfer(
                fuel_tx::ContractId::from([1u8; 32]),
                fuel_tx::ContractId::from([2u8; 32]),
                3,
                fuel_tx::AssetId::from([4u8; 32]),
                5,
                6,
            ),
            ReceiptType::TransferOut => fuel_tx::Receipt::transfer_out(
                fuel_tx::ContractId::from([1u8; 32]),
                fuel_tx::Address::from([2u8; 32]),
                3,
                fuel_tx::AssetId::from([4u8; 32]),
                5,
                6,
            ),
            ReceiptType::ScriptResult => fuel_tx::Receipt::script_result(
                fuel_tx::ScriptExecutionResult::Success,
                1,
            ),
            ReceiptType::MessageOut => fuel_tx::Receipt::message_out(
                &Default::default(),
                1,
                fuel_tx::Address::from([2u8; 32]),
                fuel_tx::Address::from([3u8; 32]),
                4,
                vec![5; 30],
            ),
            ReceiptType::Mint => fuel_tx::Receipt::mint(
                fuel_tx::Bytes32::from([1u8; 32]),
                fuel_tx::ContractId::from([2u8; 32]),
                3,
                4,
                5,
            ),
            ReceiptType::Burn => fuel_tx::Receipt::burn(
                fuel_tx::Bytes32::from([1u8; 32]),
                fuel_tx::ContractId::from([2u8; 32]),
                3,
                4,
                5,
            ),
        };
        receipts.push(receipt);
    }
    receipts
}
