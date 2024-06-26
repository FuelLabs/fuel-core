use crate::client::schema::{
    schema,
    tx::{
        TransactionStatus,
        TxIdArgs,
    },
    Address,
    AssetId,
    BlobId,
    Bytes32,
    ConnectionArgs,
    ContractId,
    ConversionError,
    HexString,
    Nonce,
    PageInfo,
    Salt,
    TransactionId,
    TxPointer,
    UtxoId,
    U16,
    U32,
    U64,
};
use core::convert::{
    TryFrom,
    TryInto,
};
use fuel_core_types::{
    fuel_tx::{
        self,
        field::ReceiptsRoot,
        input,
        output,
        policies::PolicyType,
        BlobBody,
        StorageSlot,
        UploadBody,
    },
    fuel_types,
};
use itertools::Itertools;

/// Retrieves the transaction in opaque form
#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "TxIdArgs"
)]
pub struct TransactionQuery {
    #[arguments(id: $ id)]
    pub transaction: Option<Transaction>,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "ConnectionArgs"
)]
pub struct TransactionsQuery {
    #[arguments(after: $ after, before: $ before, first: $ first, last: $ last)]
    pub transactions: TransactionConnection,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct TransactionConnection {
    pub edges: Vec<TransactionEdge>,
    pub page_info: PageInfo,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct TransactionEdge {
    pub cursor: String,
    pub node: Transaction,
}

/// The `Transaction` schema is a combination of all fields available in
/// the `fuel_tx::Transaction` from each variant plus some additional
/// data from helper functions that are often fetched by the user.
#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct Transaction {
    /// The field of the `Transaction::Script` type.
    pub script_gas_limit: Option<U64>,
    /// The field of the `Transaction` type.
    pub id: TransactionId,
    /// The field of the `Transaction::Mint`.
    pub tx_pointer: Option<TxPointer>,
    /// The list of all `AssetId` from the inputs of the transaction.
    ///
    /// The result of a `input_asset_ids()` helper function is stored here.
    /// It is not an original field of the `Transaction`.
    pub input_asset_ids: Option<Vec<AssetId>>,
    /// The list of all contracts from the inputs of the transaction.
    ///
    /// The result of a `input_contracts()` helper function is stored here.
    /// It is not an original field of the `Transaction`.
    pub input_contracts: Option<Vec<ContractId>>,
    /// The field of the `Transaction::Mint` transaction.
    pub input_contract: Option<InputContract>,
    /// The field of the `Transaction` type.
    pub inputs: Option<Vec<Input>>,
    /// It is `true` for `Transaction::Script`.
    ///
    /// The result of a `is_script()` helper function is stored here.
    /// It is not an original field of the `Transaction`.
    pub is_script: bool,
    /// It is `true` for `Transaction::Create`.
    ///
    /// The result of a `is_create()` helper function is stored here.
    /// It is not an original field of the `Transaction`.
    pub is_create: bool,
    /// It is `true` for `Transaction::Mint`.
    ///
    /// The result of a `is_mint()` helper function is stored here.
    /// It is not an original field of the `Transaction`.
    pub is_mint: bool,
    /// It is `true` for `Transaction::Upgrade`.
    ///
    /// The result of a `is_upgrade()` helper function is stored here.
    /// It is not an original field of the `Transaction`.
    pub is_upgrade: bool,
    /// It is `true` for `Transaction::Upload`.
    ///
    /// The result of a `is_upload()` helper function is stored here.
    /// It is not an original field of the `Transaction`.
    pub is_upload: bool,
    /// It is `true` for `Transaction::Blob`.
    ///
    /// The result of a `is_blob()` helper function is stored here.
    /// It is not an original field of the `Transaction`.
    pub is_blob: bool,
    /// The field of the `Transaction` type.
    pub outputs: Vec<Output>,
    /// The field of the `Transaction::Mint`.
    pub output_contract: Option<ContractOutput>,
    /// The field of the `Transaction::Mint`.
    pub mint_amount: Option<U64>,
    /// The field of the `Transaction::Mint`.
    pub mint_asset_id: Option<AssetId>,
    /// The field of the `Transaction::Mint`.
    pub mint_gas_price: Option<U64>,
    /// The field of the `Transaction::Script`.
    pub receipts_root: Option<Bytes32>,
    /// The status of the transaction fetched from the database.
    pub status: Option<TransactionStatus>,
    /// The field of the `Transaction::Script` and `Transaction::Create`.
    pub witnesses: Option<Vec<HexString>>,
    /// The field of the `Transaction::Script`.
    pub script: Option<HexString>,
    /// The field of the `Transaction::Script`.
    pub script_data: Option<HexString>,
    /// The field of the `Transaction::Script` and `Transaction::Create`.
    pub policies: Option<Policies>,
    /// The field of the `Transaction::Create`.
    pub salt: Option<Salt>,
    /// The field of the `Transaction::Create`.
    pub storage_slots: Option<Vec<HexString>>,
    /// The field of the `Transaction::Create` or `Transaction::Upload`.
    pub bytecode_witness_index: Option<U16>,
    /// The field of the `Transaction::Upload`.
    pub bytecode_root: Option<Bytes32>,
    /// The field of the `Transaction::Upload`.
    pub subsection_index: Option<U16>,
    /// The field of the `Transaction::Upload`.
    pub subsections_number: Option<U16>,
    /// The field of the `Transaction::Upload`.
    pub proof_set: Option<Vec<Bytes32>>,
    /// The field of the `Transaction::Upgrade`.
    pub upgrade_purpose: Option<UpgradePurpose>,
    /// The field of the `Transaction::Blob`.
    pub blob_id: Option<BlobId>,
}

impl TryFrom<Transaction> for fuel_tx::Transaction {
    type Error = ConversionError;

    fn try_from(tx: Transaction) -> Result<Self, Self::Error> {
        let tx = if tx.is_script {
            let mut script = fuel_tx::Transaction::script(
                tx.script_gas_limit
                    .ok_or_else(|| {
                        ConversionError::MissingField("script_gas_limit".to_string())
                    })?
                    .into(),
                tx.script
                    .ok_or_else(|| ConversionError::MissingField("script".to_string()))?
                    .into(),
                tx.script_data
                    .ok_or_else(|| {
                        ConversionError::MissingField("script_data".to_string())
                    })?
                    .into(),
                tx.policies
                    .ok_or_else(|| ConversionError::MissingField("policies".to_string()))?
                    .into(),
                tx.inputs
                    .ok_or_else(|| ConversionError::MissingField("inputs".to_string()))?
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<fuel_tx::Input>, ConversionError>>()?,
                tx.outputs
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<fuel_tx::Output>, ConversionError>>()?,
                tx.witnesses
                    .ok_or_else(|| {
                        ConversionError::MissingField("witnesses".to_string())
                    })?
                    .into_iter()
                    .map(|w| w.0 .0.into())
                    .collect(),
            );
            *script.receipts_root_mut() = tx
                .receipts_root
                .ok_or_else(|| {
                    ConversionError::MissingField("receipts_root".to_string())
                })?
                .into();
            script.into()
        } else if tx.is_create {
            let create = fuel_tx::Transaction::create(
                tx.bytecode_witness_index
                    .ok_or_else(|| {
                        ConversionError::MissingField(
                            "bytecode_witness_index".to_string(),
                        )
                    })?
                    .into(),
                tx.policies
                    .ok_or_else(|| ConversionError::MissingField("policies".to_string()))?
                    .into(),
                tx.salt
                    .ok_or_else(|| ConversionError::MissingField("salt".to_string()))?
                    .into(),
                tx.storage_slots
                    .ok_or_else(|| {
                        ConversionError::MissingField("storage_slots".to_string())
                    })?
                    .into_iter()
                    .map(|slot| {
                        if slot.0 .0.len() != 64 {
                            return Err(ConversionError::BytesLength);
                        }
                        let key = &slot.0 .0[0..32];
                        let value = &slot.0 .0[32..];
                        Ok(StorageSlot::new(
                            // unwrap is safe because length is checked
                            fuel_types::Bytes32::try_from(key)
                                .map_err(|_| ConversionError::BytesLength)?,
                            fuel_types::Bytes32::try_from(value)
                                .map_err(|_| ConversionError::BytesLength)?,
                        ))
                    })
                    .try_collect()?,
                tx.inputs
                    .ok_or_else(|| ConversionError::MissingField("inputs".to_string()))?
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<fuel_tx::Input>, ConversionError>>()?,
                tx.outputs
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<fuel_tx::Output>, ConversionError>>()?,
                tx.witnesses
                    .ok_or_else(|| {
                        ConversionError::MissingField("witnesses".to_string())
                    })?
                    .into_iter()
                    .map(|w| w.0 .0.into())
                    .collect(),
            );
            create.into()
        } else if tx.is_mint {
            let tx_pointer: fuel_tx::TxPointer = tx
                .tx_pointer
                .ok_or_else(|| ConversionError::MissingField("tx_pointer".to_string()))?
                .into();
            let mint = fuel_tx::Transaction::mint(
                tx_pointer,
                tx.input_contract
                    .ok_or_else(|| {
                        ConversionError::MissingField("input_contract".to_string())
                    })?
                    .into(),
                tx.output_contract
                    .ok_or_else(|| {
                        ConversionError::MissingField("output_contract".to_string())
                    })?
                    .try_into()?,
                tx.mint_amount
                    .ok_or_else(|| {
                        ConversionError::MissingField("mint_amount".to_string())
                    })?
                    .into(),
                tx.mint_asset_id
                    .ok_or_else(|| {
                        ConversionError::MissingField("mint_asset_id".to_string())
                    })?
                    .into(),
                tx.mint_gas_price
                    .ok_or_else(|| {
                        ConversionError::MissingField("mint_gas_price".to_string())
                    })?
                    .into(),
            );
            mint.into()
        } else if tx.is_upgrade {
            let tx = fuel_tx::Transaction::upgrade(
                tx.upgrade_purpose
                    .ok_or_else(|| {
                        ConversionError::MissingField("upgrade_purpose".to_string())
                    })?
                    .try_into()?,
                tx.policies
                    .ok_or_else(|| ConversionError::MissingField("policies".to_string()))?
                    .into(),
                tx.inputs
                    .ok_or_else(|| ConversionError::MissingField("inputs".to_string()))?
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<fuel_tx::Input>, ConversionError>>()?,
                tx.outputs
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<fuel_tx::Output>, ConversionError>>()?,
                tx.witnesses
                    .ok_or_else(|| {
                        ConversionError::MissingField("witnesses".to_string())
                    })?
                    .into_iter()
                    .map(|w| w.0 .0.into())
                    .collect(),
            );
            tx.into()
        } else if tx.is_upload {
            let tx = fuel_tx::Transaction::upload(
                UploadBody {
                    root: tx
                        .bytecode_root
                        .ok_or_else(|| {
                            ConversionError::MissingField("bytecode_root".to_string())
                        })?
                        .into(),
                    witness_index: tx
                        .bytecode_witness_index
                        .ok_or_else(|| {
                            ConversionError::MissingField("witness_index".to_string())
                        })?
                        .into(),
                    subsection_index: tx
                        .subsection_index
                        .ok_or_else(|| {
                            ConversionError::MissingField("subsection_index".to_string())
                        })?
                        .into(),
                    subsections_number: tx
                        .subsections_number
                        .ok_or_else(|| {
                            ConversionError::MissingField(
                                "subsections_number".to_string(),
                            )
                        })?
                        .into(),
                    proof_set: tx
                        .proof_set
                        .ok_or_else(|| {
                            ConversionError::MissingField("proof_set".to_string())
                        })?
                        .into_iter()
                        .map(|w| w.0 .0)
                        .collect(),
                },
                tx.policies
                    .ok_or_else(|| ConversionError::MissingField("policies".to_string()))?
                    .into(),
                tx.inputs
                    .ok_or_else(|| ConversionError::MissingField("inputs".to_string()))?
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<fuel_tx::Input>, ConversionError>>()?,
                tx.outputs
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<fuel_tx::Output>, ConversionError>>()?,
                tx.witnesses
                    .ok_or_else(|| {
                        ConversionError::MissingField("witnesses".to_string())
                    })?
                    .into_iter()
                    .map(|w| w.0 .0.into())
                    .collect(),
            );
            tx.into()
        } else if tx.is_blob {
            let tx = fuel_tx::Transaction::blob(
                BlobBody {
                    id: tx
                        .blob_id
                        .ok_or_else(|| {
                            ConversionError::MissingField("blob_id".to_string())
                        })?
                        .into(),
                    witness_index: tx
                        .bytecode_witness_index
                        .ok_or_else(|| {
                            ConversionError::MissingField("witness_index".to_string())
                        })?
                        .into(),
                },
                tx.policies
                    .ok_or_else(|| ConversionError::MissingField("policies".to_string()))?
                    .into(),
                tx.inputs
                    .ok_or_else(|| ConversionError::MissingField("inputs".to_string()))?
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<fuel_tx::Input>, ConversionError>>()?,
                tx.outputs
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<fuel_tx::Output>, ConversionError>>()?,
                tx.witnesses
                    .ok_or_else(|| {
                        ConversionError::MissingField("witnesses".to_string())
                    })?
                    .into_iter()
                    .map(|w| w.0 .0.into())
                    .collect(),
            );
            tx.into()
        } else {
            return Err(ConversionError::UnknownVariant("Transaction"));
        };

        // This `match` block is added here to enforce compilation error if a new variant
        // is added into the `fuel_tx::Transaction` enum.
        //
        // If you face a compilation error, please update the code above and add a new variant below.
        match tx {
            fuel_tx::Transaction::Script(_) => {}
            fuel_tx::Transaction::Create(_) => {}
            fuel_tx::Transaction::Mint(_) => {}
            fuel_tx::Transaction::Upgrade(_) => {}
            fuel_tx::Transaction::Upload(_) => {}
            fuel_tx::Transaction::Blob(_) => {}
        };

        Ok(tx)
    }
}

#[derive(cynic::InlineFragments, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum Input {
    InputCoin(InputCoin),
    InputContract(InputContract),
    InputMessage(InputMessage),
    #[cynic(fallback)]
    Unknown,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct InputCoin {
    pub utxo_id: UtxoId,
    pub owner: Address,
    pub amount: U64,
    pub asset_id: AssetId,
    pub tx_pointer: TxPointer,
    pub witness_index: i32,
    pub predicate_gas_used: U64,
    pub predicate: HexString,
    pub predicate_data: HexString,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct InputContract {
    pub utxo_id: UtxoId,
    pub balance_root: Bytes32,
    pub state_root: Bytes32,
    pub tx_pointer: TxPointer,
    pub contract_id: ContractId,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct InputMessage {
    sender: Address,
    recipient: Address,
    amount: U64,
    nonce: Nonce,
    witness_index: U16,
    predicate_gas_used: U64,
    data: HexString,
    predicate: HexString,
    predicate_data: HexString,
}

impl TryFrom<Input> for fuel_tx::Input {
    type Error = ConversionError;

    fn try_from(input: Input) -> Result<fuel_tx::Input, Self::Error> {
        Ok(match input {
            Input::InputCoin(coin) => {
                if coin.predicate.0 .0.is_empty() {
                    fuel_tx::Input::coin_signed(
                        coin.utxo_id.into(),
                        coin.owner.into(),
                        coin.amount.into(),
                        coin.asset_id.into(),
                        coin.tx_pointer.into(),
                        coin.witness_index.try_into()?,
                    )
                } else {
                    fuel_tx::Input::coin_predicate(
                        coin.utxo_id.into(),
                        coin.owner.into(),
                        coin.amount.into(),
                        coin.asset_id.into(),
                        coin.tx_pointer.into(),
                        coin.predicate_gas_used.into(),
                        coin.predicate.into(),
                        coin.predicate_data.into(),
                    )
                }
            }
            Input::InputContract(contract) => fuel_tx::Input::Contract(contract.into()),
            Input::InputMessage(message) => {
                match (
                    message.data.0 .0.is_empty(),
                    message.predicate.0 .0.is_empty(),
                ) {
                    (true, true) => Self::message_coin_signed(
                        message.sender.into(),
                        message.recipient.into(),
                        message.amount.into(),
                        message.nonce.into(),
                        message.witness_index.into(),
                    ),
                    (true, false) => Self::message_coin_predicate(
                        message.sender.into(),
                        message.recipient.into(),
                        message.amount.into(),
                        message.nonce.into(),
                        message.predicate_gas_used.into(),
                        message.predicate.into(),
                        message.predicate_data.into(),
                    ),
                    (false, true) => Self::message_data_signed(
                        message.sender.into(),
                        message.recipient.into(),
                        message.amount.into(),
                        message.nonce.into(),
                        message.witness_index.into(),
                        message.data.into(),
                    ),
                    (false, false) => Self::message_data_predicate(
                        message.sender.into(),
                        message.recipient.into(),
                        message.amount.into(),
                        message.nonce.into(),
                        message.predicate_gas_used.into(),
                        message.data.into(),
                        message.predicate.into(),
                        message.predicate_data.into(),
                    ),
                }
            }
            Input::Unknown => return Err(Self::Error::UnknownVariant("Input")),
        })
    }
}

#[derive(cynic::InlineFragments, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum Output {
    CoinOutput(CoinOutput),
    ContractOutput(ContractOutput),
    ChangeOutput(ChangeOutput),
    VariableOutput(VariableOutput),
    ContractCreated(ContractCreated),
    #[cynic(fallback)]
    Unknown,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct CoinOutput {
    pub to: Address,
    pub amount: U64,
    pub asset_id: AssetId,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ChangeOutput {
    pub to: Address,
    pub amount: U64,
    pub asset_id: AssetId,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct VariableOutput {
    pub to: Address,
    pub amount: U64,
    pub asset_id: AssetId,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ContractOutput {
    pub input_index: U16,
    pub balance_root: Bytes32,
    pub state_root: Bytes32,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ContractCreated {
    contract: ContractId,
    state_root: Bytes32,
}

impl TryFrom<Output> for fuel_tx::Output {
    type Error = ConversionError;

    fn try_from(value: Output) -> Result<Self, Self::Error> {
        Ok(match value {
            Output::CoinOutput(coin) => Self::Coin {
                to: coin.to.into(),
                amount: coin.amount.into(),
                asset_id: coin.asset_id.into(),
            },
            Output::ContractOutput(contract) => Self::Contract(contract.try_into()?),
            Output::ChangeOutput(change) => Self::Change {
                to: change.to.into(),
                amount: change.amount.into(),
                asset_id: change.asset_id.into(),
            },
            Output::VariableOutput(variable) => Self::Variable {
                to: variable.to.into(),
                amount: variable.amount.into(),
                asset_id: variable.asset_id.into(),
            },
            Output::ContractCreated(contract) => Self::ContractCreated {
                contract_id: contract.contract.into(),
                state_root: contract.state_root.into(),
            },
            Output::Unknown => return Err(Self::Error::UnknownVariant("Output")),
        })
    }
}

impl From<InputContract> for input::contract::Contract {
    fn from(contract: InputContract) -> Self {
        input::contract::Contract {
            utxo_id: contract.utxo_id.into(),
            balance_root: contract.balance_root.into(),
            state_root: contract.state_root.into(),
            tx_pointer: contract.tx_pointer.into(),
            contract_id: contract.contract_id.into(),
        }
    }
}

impl TryFrom<ContractOutput> for output::contract::Contract {
    type Error = ConversionError;

    fn try_from(contract: ContractOutput) -> Result<Self, Self::Error> {
        Ok(output::contract::Contract {
            input_index: contract.input_index.into(),
            balance_root: contract.balance_root.into(),
            state_root: contract.state_root.into(),
        })
    }
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct Policies {
    pub tip: Option<U64>,
    pub maturity: Option<U32>,
    pub witness_limit: Option<U64>,
    pub max_fee: Option<U64>,
}

impl From<Policies> for fuel_tx::policies::Policies {
    fn from(value: Policies) -> Self {
        let mut policies = fuel_tx::policies::Policies::new();
        policies.set(PolicyType::Tip, value.tip.map(Into::into));
        policies.set(
            PolicyType::Maturity,
            value.maturity.map(|maturity| maturity.0 as u64),
        );
        policies.set(
            PolicyType::WitnessLimit,
            value.witness_limit.map(Into::into),
        );
        policies.set(PolicyType::MaxFee, value.max_fee.map(Into::into));
        policies
    }
}

#[derive(cynic::InlineFragments, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub enum UpgradePurpose {
    ConsensusParameters(ConsensusParametersPurpose),
    StateTransition(StateTransitionPurpose),
    #[cynic(fallback)]
    Unknown,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct ConsensusParametersPurpose {
    witness_index: U16,
    checksum: Bytes32,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct StateTransitionPurpose {
    root: Bytes32,
}

impl TryFrom<UpgradePurpose> for fuel_tx::UpgradePurpose {
    type Error = ConversionError;

    fn try_from(value: UpgradePurpose) -> Result<Self, Self::Error> {
        match value {
            UpgradePurpose::ConsensusParameters(v) => {
                Ok(fuel_tx::UpgradePurpose::ConsensusParameters {
                    witness_index: v.witness_index.into(),
                    checksum: v.checksum.into(),
                })
            }
            UpgradePurpose::StateTransition(v) => {
                Ok(fuel_tx::UpgradePurpose::StateTransition {
                    root: v.root.into(),
                })
            }
            UpgradePurpose::Unknown => Err(Self::Error::UnknownVariant("UpgradePurpose")),
        }
    }
}
