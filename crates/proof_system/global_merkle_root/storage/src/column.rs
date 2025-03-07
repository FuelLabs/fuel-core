/// Almost in the case of all tables we need to prove exclusion of entries,
/// in the case of malicious block.
#[repr(u32)]
#[derive(
    Copy,
    Clone,
    Debug,
    strum_macros::EnumCount,
    strum_macros::IntoStaticStr,
    PartialEq,
    Eq,
    enum_iterator::Sequence,
    Hash,
    num_enum::TryFromPrimitive,
)]
pub enum TableColumn {
    /// Only can be proved with global root or a double spend proof.
    ContractsRawCode = 0,
    /// Only can be proved with global root or a double spend proof.
    ContractsLatestUtxo = 1,
    /// Only can be proved with global root or a double spend proof.
    Coins = 2,
    /// Only can be proved with list of events processed during the block
    /// and compared with the `event_inbox_root` in the block header.
    Messages = 3,
    /// We need to prove that the transaction isn't included in the table.
    /// Only can be proved with global root or a double spend proof.
    ProcessedTransactions = 4,
    /// Only can be proved with global root or a double spend proof.
    ConsensusParametersVersions = 5,
    /// Only can be proved with global root or a double spend proof.
    StateTransitionBytecodeVersions = 6,
    /// Only can be proved with global root or a double spend proof.
    UploadedBytecodes = 7,
    /// Only can be proved with global root or a double spend proof.
    Blobs = 8,
}

impl TableColumn {
    /// The total count of variants in the enum.
    pub const COUNT: usize = <Self as strum::EnumCount>::COUNT;
}

impl fuel_core_storage::merkle::column::AsU32 for TableColumn {
    fn as_u32(&self) -> u32 {
        *self as u32
    }
}
