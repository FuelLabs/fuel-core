//! The crate `fuel-core-storage` contains storage types, primitives, tables used by `fuel-core`.
//! This crate doesn't contain the actual implementation of the storage. It works around the
//! `Database` and is used by services to provide a default implementation. Primitives
//! defined here are used by services but are flexible enough to customize the
//! logic when the `Database` is known.

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
// #![deny(unused_crate_dependencies)]
// #![deny(missing_docs)]
// #![deny(warnings)]

#[cfg(feature = "alloc")]
extern crate alloc;

use crate::{
    column::Columns,
    merkle::{
        DummyStorage,
        Merklized,
    },
};
use alloc::borrow::Cow;
use fuel_core_storage::{
    transactional::{
        Changes,
        ConflictPolicy,
        StorageTransaction,
    },
    StorageAsMut,
    StorageMutate,
};
use fuel_core_types::{
    blockchain::block::Block,
    entities::{
        coins::coin::CompressedCoinV1,
        contract::ContractUtxoInfo,
    },
    fuel_asm::Word,
    fuel_tx::{
        field::{
            InputContract,
            Inputs,
            OutputContract,
            Outputs,
        },
        input,
        input::{
            coin::{
                CoinPredicate,
                CoinSigned,
            },
            message::{
                MessageCoinPredicate,
                MessageCoinSigned,
                MessageDataPredicate,
                MessageDataSigned,
            },
        },
        Input,
        Output,
        Transaction,
        TxPointer,
        UniqueIdentifier,
        UtxoId,
    },
    fuel_types::ChainId,
    services::executor::{
        Error as ExecutorError,
        TransactionValidityError,
    },
};

pub mod column;
pub mod merkle;
pub mod structured_storage;

pub type ContractsRawCode = Merklized<fuel_core_storage::tables::ContractsRawCode>;
pub type ContractsLatestUtxo = Merklized<fuel_core_storage::tables::ContractsLatestUtxo>;
pub type Coins = Merklized<fuel_core_storage::tables::Coins>;
pub type Messages = Merklized<fuel_core_storage::tables::Messages>;
pub type ProcessedTransactions =
    Merklized<fuel_core_storage::tables::ProcessedTransactions>;
pub type ConsensusParametersVersions =
    Merklized<fuel_core_storage::tables::ConsensusParametersVersions>;
pub type StateTransitionBytecodeVersions =
    Merklized<fuel_core_storage::tables::StateTransitionBytecodeVersions>;
pub type UploadedBytecodes = Merklized<fuel_core_storage::tables::UploadedBytecodes>;
pub type Blobs = Merklized<fuel_core_storage::tables::BlobData>;

fn process_block(block: &Block) -> anyhow::Result<()> {
    // TODO: Get chain id from the consensus parameters, or hardcode it.
    let chain_id = ChainId::default();
    let mut storage = StorageTransaction::transaction(
        DummyStorage::<Columns>::new(),
        ConflictPolicy::Fail,
        Changes::default(),
    );

    let block_height = *block.header().height();

    for (tx_idx, tx) in block.transactions().iter().enumerate() {
        let tx_id = tx.id(&chain_id);
        let tx_idx: u16 =
            u16::try_from(tx_idx).map_err(|_| ExecutorError::TooManyTransactions)?;
        let tx_pointer = TxPointer::new(block_height, tx_idx);

        let inputs = tx.inputs();
        for input in inputs.as_ref() {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                    // If you have an unknown error during compilation, you can force compiler
                    // to say exact what is the problem by the command below.
                    // let _: &mut dyn StorageMutate<
                    //     Coins,
                    //     Error = fuel_core_storage::Error,
                    // > = &mut storage;
                    storage.storage_as_mut::<Coins>().remove(utxo_id)?;
                }
                Input::Contract(_) => {
                    // Do nothing, since we are interested in output values
                }
                Input::MessageCoinSigned(MessageCoinSigned { nonce, .. })
                | Input::MessageCoinPredicate(MessageCoinPredicate { nonce, .. }) => {
                    storage.storage_as_mut::<Messages>().remove(nonce)?;
                }
                // The messages below are retryable, it means that if execution failed,
                // message is not spend.
                Input::MessageDataSigned(MessageDataSigned { nonce, .. })
                | Input::MessageDataPredicate(MessageDataPredicate { nonce, .. }) => {
                    // TODO: Figure out how to know the status of the execution.
                    //  We definitely can do it via providing all receipts and verifying
                    //  the script root. But maybe we have less expensive way.
                    let success_status = false;
                    if success_status {
                        storage.storage_as_mut::<Messages>().remove(nonce)?;
                    }
                }
            }
        }

        for (output_index, output) in tx.outputs().as_ref().iter().enumerate() {
            let output_index =
                u16::try_from(output_index).map_err(|_| ExecutorError::TooManyOutputs)?;
            let utxo_id = UtxoId::new(tx_id, output_index);

            match output {
                Output::Coin {
                    to,
                    amount,
                    asset_id,
                }
                | Output::Change {
                    to,
                    amount,
                    asset_id,
                }
                | Output::Variable {
                    to,
                    amount,
                    asset_id,
                } => {
                    // Only insert a coin output if it has some amount.
                    // This is because variable or transfer outputs won't have any value
                    // if there's a revert or panic and shouldn't be added to the utxo set.
                    if *amount > Word::MIN {
                        let coin = CompressedCoinV1 {
                            owner: *to,
                            amount: *amount,
                            asset_id: *asset_id,
                            tx_pointer,
                        }
                        .into();

                        storage.storage::<Coins>().insert(&utxo_id, &coin)?;
                    }
                }
                Output::Contract(contract) => {
                    if let Some(Input::Contract(input::contract::Contract {
                        contract_id,
                        ..
                    })) = inputs.get(contract.input_index as usize)
                    {
                        storage.storage::<ContractsLatestUtxo>().insert(
                            contract_id,
                            &ContractUtxoInfo::V1((utxo_id, tx_pointer).into()),
                        )?;
                    } else {
                        Err(ExecutorError::TransactionValidity(
                            TransactionValidityError::InvalidContractInputIndex(utxo_id),
                        ))?;
                    }

                    // TODO: Think about it more: Maybe we want to create a global Merkle root
                    //  of roots for the contract.
                }
                Output::ContractCreated { contract_id, .. } => {
                    storage.storage::<ContractsLatestUtxo>().insert(
                        contract_id,
                        &ContractUtxoInfo::V1((utxo_id, tx_pointer).into()),
                    )?;
                }
            }
        }

        // TODO: Process the type of the transaction
    }

    Ok(())
}

pub trait TransactionExt {
    fn inputs(&self) -> Cow<Vec<Input>>;

    fn outputs(&self) -> Cow<Vec<Output>>;
}

impl TransactionExt for Transaction {
    fn inputs(&self) -> Cow<Vec<Input>> {
        match self {
            Transaction::Script(tx) => Cow::Borrowed(tx.inputs()),
            Transaction::Create(tx) => Cow::Borrowed(tx.inputs()),
            Transaction::Mint(tx) => {
                Cow::Owned(vec![Input::Contract(tx.input_contract().clone())])
            }
            Transaction::Upgrade(tx) => Cow::Borrowed(tx.inputs()),
            Transaction::Upload(tx) => Cow::Borrowed(tx.inputs()),
            Transaction::Blob(tx) => Cow::Borrowed(tx.inputs()),
        }
    }

    fn outputs(&self) -> Cow<Vec<Output>> {
        match self {
            Transaction::Script(tx) => Cow::Borrowed(tx.outputs()),
            Transaction::Create(tx) => Cow::Borrowed(tx.outputs()),
            Transaction::Mint(tx) => {
                Cow::Owned(vec![Output::Contract(tx.output_contract().clone())])
            }
            Transaction::Upgrade(tx) => Cow::Borrowed(tx.outputs()),
            Transaction::Upload(tx) => Cow::Borrowed(tx.outputs()),
            Transaction::Blob(tx) => Cow::Borrowed(tx.outputs()),
        }
    }
}

#[test]
fn dummy() {}
