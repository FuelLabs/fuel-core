use crate::{
    column::Column,
    error::{
        InvalidBlock,
        InvariantViolation,
    },
    Blobs,
    Coins,
    ConsensusParametersVersions,
    ContractsLatestUtxo,
    ContractsRawCode,
    Messages,
    ProcessedTransactions,
    StateTransitionBytecodeVersions,
    UploadedBytecodes,
};
use alloc::{
    borrow::Cow,
    vec,
    vec::Vec,
};
use fuel_core_storage::{
    kv_store::KeyValueInspect,
    transactional::StorageTransaction,
    StorageAsMut,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::{
        block::Block,
        header::{
            ConsensusParametersVersion,
            StateTransitionBytecodeVersion,
        },
    },
    entities::{
        coins::coin::CompressedCoinV1,
        contract::ContractUtxoInfo,
    },
    fuel_asm::Word,
    fuel_tx::{
        field::{
            BytecodeRoot,
            BytecodeWitnessIndex,
            ChargeableBody,
            InputContract,
            Inputs,
            OutputContract,
            Outputs,
            UpgradePurpose as _,
            Witnesses,
        },
        input::{
            self,
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
        output,
        Address,
        AssetId,
        Blob,
        BlobBody,
        Create,
        Input,
        Output,
        Transaction,
        TxId,
        TxPointer,
        UniqueIdentifier,
        Upgrade,
        UpgradeMetadata,
        UpgradePurpose,
        Upload,
        UtxoId,
    },
    fuel_types::{
        BlockHeight,
        ChainId,
    },
    fuel_vm::UploadedBytecode,
};

/// Main entrypoint to the update functionality
pub trait UpdateMerkleizedTables {
    /// Process a block and update all merkleized tables
    fn update_merkleized_tables(
        &mut self,
        chain_id: ChainId,
        block: &Block,
    ) -> crate::Result<&mut Self>;
}

impl<Storage> UpdateMerkleizedTables for StorageTransaction<Storage>
where
    Storage: KeyValueInspect<Column = Column>,
{
    #[tracing::instrument(skip(self, block))]
    fn update_merkleized_tables(
        &mut self,
        chain_id: ChainId,
        block: &Block,
    ) -> crate::Result<&mut Self> {
        let mut update_transaction = UpdateMerkleizedTablesTransaction {
            chain_id,
            storage: self,
            latest_state_transition_bytecode_version: block
                .header()
                .state_transition_bytecode_version(),
            latest_consensus_parameters_version: block
                .header()
                .consensus_parameters_version(),
        };

        update_transaction.process_block(block)?;

        Ok(self)
    }
}

struct UpdateMerkleizedTablesTransaction<'a, Storage> {
    chain_id: ChainId,
    storage: &'a mut StorageTransaction<Storage>,
    latest_consensus_parameters_version: ConsensusParametersVersion,
    latest_state_transition_bytecode_version: StateTransitionBytecodeVersion,
}

impl<'a, Storage> UpdateMerkleizedTablesTransaction<'a, Storage>
where
    Storage: KeyValueInspect<Column = Column>,
{
    #[tracing::instrument(skip(self, block))]
    pub fn process_block(&mut self, block: &Block) -> crate::Result<()> {
        let block_height = *block.header().height();
        tracing::info!(%block_height, "processing block");

        for (tx_idx, tx) in block.transactions().iter().enumerate() {
            let tx_idx: u16 =
                u16::try_from(tx_idx).map_err(|_| InvalidBlock::TooManyTransactions)?;
            self.process_transaction(block_height, tx_idx, tx)?;
        }

        Ok(())
    }

    #[tracing::instrument(skip(self, tx))]
    fn process_transaction(
        &mut self,
        block_height: BlockHeight,
        tx_idx: u16,
        tx: &Transaction,
    ) -> crate::Result<()> {
        let tx_id = tx.id(&self.chain_id);
        let inputs = tx.inputs();

        for input in inputs.iter() {
            self.process_input(input)?;
        }

        let tx_pointer = TxPointer::new(block_height, tx_idx);
        for (output_index, output) in tx.outputs().iter().enumerate() {
            let output_index =
                u16::try_from(output_index).map_err(|_| InvalidBlock::TooManyOutputs)?;

            let utxo_id = UtxoId::new(tx_id, output_index);
            self.process_output(tx_pointer, utxo_id, &inputs, output)?;
        }

        match tx {
            Transaction::Create(tx) => self.process_create_transaction(tx)?,
            Transaction::Upgrade(tx) => self.process_upgrade_transaction(tx)?,
            Transaction::Upload(tx) => self.process_upload_transaction(tx)?,
            Transaction::Blob(tx) => self.process_blob_transaction(tx)?,
            Transaction::Script(_) | Transaction::Mint(_) => (),
        };

        self.store_processed_transaction(tx_id)?;

        Ok(())
    }

    #[tracing::instrument(skip(self, input))]
    fn process_input(&mut self, input: &Input) -> crate::Result<()> {
        match input {
            Input::CoinSigned(CoinSigned { utxo_id, .. })
            | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                tracing::debug!(%utxo_id, "removing coin");
                self.storage.storage_as_mut::<Coins>().remove(utxo_id)?;
            }
            Input::Contract(_) => {
                // Do nothing, since we are interested in output values
            }
            Input::MessageCoinSigned(MessageCoinSigned { nonce, .. })
            | Input::MessageCoinPredicate(MessageCoinPredicate { nonce, .. }) => {
                tracing::debug!(%nonce, "removing message coin");
                self.storage.storage_as_mut::<Messages>().remove(nonce)?;
            }
            // The messages below are retryable, it means that if execution failed,
            // message is not spend.
            Input::MessageDataSigned(MessageDataSigned { nonce, .. })
            | Input::MessageDataPredicate(MessageDataPredicate { nonce, .. }) => {
                // TODO(#2589): Figure out how to know the status of the execution.
                //  We definitely can do it via providing all receipts and verifying
                //  the script root. But maybe we have less expensive way.
                let success_status = false;
                if success_status {
                    tracing::debug!(%nonce, "removing data message");
                    self.storage.storage_as_mut::<Messages>().remove(nonce)?;
                }
            }
        }

        Ok(())
    }

    #[tracing::instrument(skip(self, inputs, output))]
    fn process_output(
        &mut self,
        tx_pointer: TxPointer,
        utxo_id: UtxoId,
        inputs: &[Input],
        output: &Output,
    ) -> crate::Result<()> {
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
                self.insert_coin_if_it_has_amount(
                    tx_pointer, utxo_id, *to, *amount, *asset_id,
                )?;
            }
            Output::Contract(contract) => {
                self.try_insert_latest_contract_utxo(
                    tx_pointer, utxo_id, inputs, *contract,
                )?;
            }
            Output::ContractCreated { contract_id, .. } => {
                tracing::debug!(%contract_id, "creating contract");
                self.storage.storage::<ContractsLatestUtxo>().insert(
                    contract_id,
                    &ContractUtxoInfo::V1((utxo_id, tx_pointer).into()),
                )?;
            }
        }
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn store_processed_transaction(&mut self, tx_id: TxId) -> crate::Result<()> {
        tracing::debug!("storing processed transaction");
        let previous_tx = self
            .storage
            .storage_as_mut::<ProcessedTransactions>()
            .replace(&tx_id, &())?;

        if previous_tx.is_some() {
            Err(InvalidBlock::DuplicateTransaction(tx_id))?
        };

        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn insert_coin_if_it_has_amount(
        &mut self,
        tx_pointer: TxPointer,
        utxo_id: UtxoId,
        owner: Address,
        amount: Word,
        asset_id: AssetId,
    ) -> crate::Result<()> {
        // Only insert a coin output if it has some amount.
        // This is because variable or transfer outputs won't have any value
        // if there's a revert or panic and shouldn't be added to the utxo set.
        if amount > Word::MIN {
            let coin = CompressedCoinV1 {
                owner,
                amount,
                asset_id,
                tx_pointer,
            }
            .into();

            tracing::debug!("storing coin");
            let None = self.storage.storage::<Coins>().replace(&utxo_id, &coin)? else {
                // We should never overwrite coins.
                return Err(InvariantViolation::CoinAlreadyExists(utxo_id).into())
            };
        }

        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn try_insert_latest_contract_utxo(
        &mut self,
        tx_pointer: TxPointer,
        utxo_id: UtxoId,
        inputs: &[Input],
        contract: output::contract::Contract,
    ) -> crate::Result<()> {
        if let Some(Input::Contract(input::contract::Contract { contract_id, .. })) =
            inputs.get(contract.input_index as usize)
        {
            tracing::debug!("storing contract UTxO");
            self.storage.storage::<ContractsLatestUtxo>().insert(
                contract_id,
                &ContractUtxoInfo::V1((utxo_id, tx_pointer).into()),
            )?;
        } else {
            tracing::warn!("invalid contract input index");
            Err(InvalidBlock::InvalidContractInputIndex(
                contract.input_index,
            ))?;
        }
        Ok(())
    }

    #[tracing::instrument(skip(self, tx))]
    fn process_create_transaction(&mut self, tx: &Create) -> crate::Result<()> {
        let bytecode_witness_index = tx.bytecode_witness_index();
        let witnesses = tx.witnesses();
        let bytecode = witnesses
            .get(usize::from(*bytecode_witness_index))
            .ok_or_else(|| InvalidBlock::MissingWitness {
                txid: tx.id(&self.chain_id),
                witness_index: usize::from(*bytecode_witness_index),
            })?
            .as_vec();

        // The Fuel specs mandate that each create transaction has exactly one output of type `Output::ContractCreated`.
        // See https://docs.fuel.network/docs/specs/tx-format/transaction/#transactioncreate
        let Some(Output::ContractCreated { contract_id, .. }) = tx
            .outputs()
            .iter()
            .find(|output| matches!(output, Output::ContractCreated { .. }))
        else {
            Err(InvalidBlock::ContractCreatedOutputNotFound)?
        };

        tracing::debug!("storing contract code");
        self.storage
            .storage_as_mut::<ContractsRawCode>()
            .insert(contract_id, bytecode)?;
        Ok(())
    }

    #[tracing::instrument(skip(self, tx))]
    fn process_upgrade_transaction(&mut self, tx: &Upgrade) -> crate::Result<()> {
        let metadata = match tx.metadata() {
            Some(metadata) => metadata.body.clone(),
            None => {
                UpgradeMetadata::compute(tx).map_err(InvalidBlock::TxValidityError)?
            }
        };

        match metadata {
            UpgradeMetadata::ConsensusParameters {
                consensus_parameters,
                calculated_checksum: _,
            } => {
                let Some(next_consensus_parameters_version) =
                    self.latest_consensus_parameters_version.checked_add(1)
                else {
                    return Err(InvalidBlock::ConsensusParametersVersionOutOfBounds.into())
                };
                self.latest_consensus_parameters_version =
                    next_consensus_parameters_version;

                tracing::debug!("storing next consensus parameters version");
                self.storage
                    .storage::<ConsensusParametersVersions>()
                    .insert(
                        &self.latest_consensus_parameters_version,
                        &consensus_parameters,
                    )?;
            }
            UpgradeMetadata::StateTransition => match tx.upgrade_purpose() {
                UpgradePurpose::ConsensusParameters { .. } => unreachable!(
                    "Upgrade with StateTransition metadata should have StateTransition purpose"
                ),
                UpgradePurpose::StateTransition { root } => {
                    let Some(next_state_transition_bytecode_version) =
                        self.latest_state_transition_bytecode_version.checked_add(1)
                    else {
                        return Err(InvalidBlock::StateTransitionBytecodeVersionOutOfBounds.into());
                    };
                    self.latest_state_transition_bytecode_version =
                        next_state_transition_bytecode_version;

                tracing::debug!("storing next state transition bytecode version");
                    self.storage
                        .storage::<StateTransitionBytecodeVersions>()
                        .insert(&self.latest_state_transition_bytecode_version, root)?;
                }
            },
        }

        Ok(())
    }

    #[tracing::instrument(skip(self, tx))]
    fn process_upload_transaction(&mut self, tx: &Upload) -> crate::Result<()> {
        let bytecode_root = *tx.bytecode_root();
        let uploaded_bytecode = self
            .storage
            .storage_as_ref::<UploadedBytecodes>()
            .get(&bytecode_root)?
            .map(Cow::into_owned)
            .unwrap_or_else(|| UploadedBytecode::Uncompleted {
                bytecode: Vec::new(),
                uploaded_subsections_number: 0,
            });

        let UploadedBytecode::Uncompleted {
            mut bytecode,
            uploaded_subsections_number,
        } = uploaded_bytecode
        else {
            Err(InvalidBlock::BytecodeAlreadyCompleted)?
        };

        let expected_subsection_index = uploaded_subsections_number;
        let subsection_index = tx.body().subsection_index;

        if expected_subsection_index != subsection_index {
            Err(InvalidBlock::WrongSubsectionIndex {
                expected: expected_subsection_index,
                observed: subsection_index,
            })?
        };

        let witness_index = usize::from(*tx.bytecode_witness_index());
        let new_bytecode =
            tx.witnesses()
                .get(witness_index)
                .ok_or(InvalidBlock::MissingWitness {
                    txid: tx.id(&self.chain_id),
                    witness_index,
                })?;

        bytecode.extend_from_slice(new_bytecode.as_ref());

        let new_uploaded_subsections_number = uploaded_subsections_number
            .checked_add(1)
            .ok_or(InvalidBlock::SubsectionNumberOverflow)?;

        let new_uploaded_bytecode =
            if new_uploaded_subsections_number == tx.body().subsections_number {
                UploadedBytecode::Completed(bytecode)
            } else {
                UploadedBytecode::Uncompleted {
                    bytecode,
                    uploaded_subsections_number: new_uploaded_subsections_number,
                }
            };

        tracing::debug!("storing uploaded bytecodes");
        self.storage
            .storage_as_mut::<UploadedBytecodes>()
            .insert(&bytecode_root, &new_uploaded_bytecode)?;

        Ok(())
    }

    #[tracing::instrument(skip(self, tx))]
    fn process_blob_transaction(&mut self, tx: &Blob) -> crate::Result<()> {
        let BlobBody {
            id: blob_id,
            witness_index,
        } = tx.body();

        let witness_index = usize::from(*witness_index);
        let blob =
            tx.witnesses()
                .get(witness_index)
                .ok_or(InvalidBlock::MissingWitness {
                    txid: tx.id(&self.chain_id),
                    witness_index,
                })?;

        tracing::debug!("storing blob");
        self.storage
            .storage::<Blobs>()
            .insert(blob_id, blob.as_ref())?;

        Ok(())
    }
}

trait TransactionInputs {
    fn inputs(&self) -> Cow<Vec<Input>>;
}

trait TransactionOutputs {
    fn outputs(&self) -> Cow<Vec<Output>>;
}

impl TransactionInputs for Transaction {
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
}

impl TransactionOutputs for Transaction {
    fn outputs(&self) -> Cow<Vec<Output>> {
        match self {
            Transaction::Script(tx) => Cow::Borrowed(tx.outputs()),
            Transaction::Create(tx) => Cow::Borrowed(tx.outputs()),
            Transaction::Mint(tx) => {
                Cow::Owned(vec![Output::Contract(*tx.output_contract())])
            }
            Transaction::Upgrade(tx) => Cow::Borrowed(tx.outputs()),
            Transaction::Upload(tx) => Cow::Borrowed(tx.outputs()),
            Transaction::Blob(tx) => Cow::Borrowed(tx.outputs()),
        }
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    use crate::test_helpers;

    use fuel_core_storage::{
        structured_storage::test::InMemoryStorage,
        transactional::{
            ReadTransaction,
            WriteTransaction,
        },
        StorageAsRef,
    };
    use fuel_core_types::{
        fuel_asm::{
            op,
            RegId,
        },
        fuel_crypto::Hasher,
        fuel_tx::{
            BlobId,
            BlobIdExt,
            Bytes32,
            ConsensusParameters,
            Contract,
            Finalizable,
            TransactionBuilder,
            UploadBody,
            Witness,
        },
        fuel_vm::CallFrame,
    };

    use rand::{
        rngs::StdRng,
        Rng,
        SeedableRng,
    };

    #[test]
    /// When encountering a transaction with a coin output,
    /// `process_output` should ensure this coin is
    /// populated in the `Coins` table.
    fn process_output__should_insert_coin() {
        let mut rng = StdRng::seed_from_u64(1337);

        // Given
        let mut storage: InMemoryStorage<Column> = InMemoryStorage::default();
        let mut storage_tx = storage.write_transaction();
        let mut storage_update_tx =
            storage_tx.construct_update_merkleized_tables_transaction();

        let tx_pointer = test_helpers::random_tx_pointer(&mut rng);
        let utxo_id = test_helpers::random_utxo_id(&mut rng);
        let inputs = vec![];

        let output_amount = rng.gen();
        let output_address = test_helpers::random_address(&mut rng);
        let output = Output::Coin {
            to: output_address,
            amount: output_amount,
            asset_id: AssetId::zeroed(),
        };

        // When
        storage_update_tx
            .process_output(tx_pointer, utxo_id, &inputs, &output)
            .unwrap();

        storage_tx.commit().unwrap();

        let inserted_coin = storage
            .read_transaction()
            .storage_as_ref::<Coins>()
            .get(&utxo_id)
            .unwrap()
            .unwrap()
            .into_owned();

        // Then
        assert_eq!(*inserted_coin.amount(), output_amount);
        assert_eq!(*inserted_coin.owner(), output_address);
    }

    #[test]
    /// When encountering a transaction with a contract created output,
    /// `process_output` should ensure an appropriate contract UTxO is
    /// populated in the `ContractsLatestUtxo` table.
    fn process_output__should_insert_latest_contract_utxo_when_contract_created() {
        let mut rng = StdRng::seed_from_u64(1337);

        // Given
        let mut storage: InMemoryStorage<Column> = InMemoryStorage::default();
        let mut storage_tx = storage.write_transaction();
        let mut storage_update_tx =
            storage_tx.construct_update_merkleized_tables_transaction();

        let tx_pointer = test_helpers::random_tx_pointer(&mut rng);
        let utxo_id = test_helpers::random_utxo_id(&mut rng);
        let inputs = vec![];

        let contract_id = test_helpers::random_contract_id(&mut rng);
        let output = Output::ContractCreated {
            contract_id,
            state_root: Bytes32::zeroed(),
        };

        // When
        storage_update_tx
            .process_output(tx_pointer, utxo_id, &inputs, &output)
            .unwrap();

        storage_tx.commit().unwrap();

        let inserted_contract_utxo = storage
            .read_transaction()
            .storage_as_ref::<ContractsLatestUtxo>()
            .get(&contract_id)
            .unwrap()
            .unwrap()
            .into_owned();

        // Then
        assert_eq!(inserted_contract_utxo.utxo_id(), &utxo_id);
    }

    #[test]
    /// When encountering a transaction with a contract output,
    /// `process_output` should ensure an appropriate contract UTxO is
    /// populated in the `ContractsLatestUtxo` table.
    fn process_output__should_update_latest_contract_utxo_when_interacting_with_contract()
    {
        let mut rng = StdRng::seed_from_u64(1337);

        // Given
        let mut storage: InMemoryStorage<Column> = InMemoryStorage::default();
        let mut storage_tx = storage.write_transaction();
        let mut storage_update_tx =
            storage_tx.construct_update_merkleized_tables_transaction();

        let tx_pointer = test_helpers::random_tx_pointer(&mut rng);
        let utxo_id = test_helpers::random_utxo_id(&mut rng);

        let contract_id = test_helpers::random_contract_id(&mut rng);
        let input_contract = input::contract::Contract {
            contract_id,
            ..Default::default()
        };
        let inputs = vec![Input::Contract(input_contract)];

        let output_contract = output::contract::Contract {
            input_index: 0,
            ..Default::default()
        };

        let output = Output::Contract(output_contract);

        // When
        storage_update_tx
            .process_output(tx_pointer, utxo_id, &inputs, &output)
            .unwrap();

        storage_tx.commit().unwrap();

        let inserted_contract_utxo = storage
            .read_transaction()
            .storage_as_ref::<ContractsLatestUtxo>()
            .get(&contract_id)
            .unwrap()
            .unwrap()
            .into_owned();

        // Then
        assert_eq!(inserted_contract_utxo.utxo_id(), &utxo_id);
    }

    #[test]
    /// When encountering a transaction with a coin input,
    /// `process_input` should ensure this coin is
    /// removed from the `Coins` table, as this coin is no longer
    /// a part of the active UTxO set.
    fn process_input__should_remove_coin() {
        let mut rng = StdRng::seed_from_u64(1337);

        // Given
        let mut storage: InMemoryStorage<Column> = InMemoryStorage::default();
        let mut storage_tx = storage.write_transaction();
        let mut storage_update_tx =
            storage_tx.construct_update_merkleized_tables_transaction();

        let output_amount = rng.gen();
        let output_address = test_helpers::random_address(&mut rng);
        let tx_pointer = test_helpers::random_tx_pointer(&mut rng);
        let utxo_id = test_helpers::random_utxo_id(&mut rng);
        let inputs = vec![];

        let output = Output::Coin {
            to: output_address,
            amount: output_amount,
            asset_id: AssetId::zeroed(),
        };

        let input = Input::CoinSigned(CoinSigned {
            utxo_id,
            ..Default::default()
        });

        // When
        storage_update_tx
            .process_output(tx_pointer, utxo_id, &inputs, &output)
            .unwrap();

        let coin_was_inserted_before_process_input = storage_update_tx
            .storage
            .storage_as_ref::<Coins>()
            .get(&utxo_id)
            .unwrap()
            .is_some();

        storage_update_tx.process_input(&input).unwrap();

        storage_tx.commit().unwrap();

        let coin_doesnt_exist_after_process_input = storage
            .read_transaction()
            .storage_as_ref::<Coins>()
            .get(&utxo_id)
            .unwrap()
            .is_none();

        // Then
        assert!(coin_was_inserted_before_process_input);
        assert!(coin_doesnt_exist_after_process_input);
    }

    #[test]
    fn process_upgrade_transaction_should_update_latest_state_transition_bytecode_version_when_interacting_with_relevant_upgrade(
    ) {
        // Given
        let new_root = Bytes32::from([1; 32]);
        let upgrade_tx = TransactionBuilder::upgrade(UpgradePurpose::StateTransition {
            root: new_root,
        })
        .finalize();

        let mut storage = InMemoryStorage::default();

        // When
        let state_transition_bytecode_version_before_upgrade = 1;
        let state_transition_bytecode_version_after_upgrade =
            state_transition_bytecode_version_before_upgrade + 1;

        let mut storage_tx = storage.write_transaction();
        let mut update_tx = storage_tx
            .construct_update_merkleized_tables_transaction_with_versions(
                state_transition_bytecode_version_before_upgrade,
                0,
            );
        update_tx.process_upgrade_transaction(&upgrade_tx).unwrap();

        let state_transition_bytecode_root_after_upgrade = storage_tx
            .storage_as_ref::<StateTransitionBytecodeVersions>()
            .get(&state_transition_bytecode_version_after_upgrade)
            .expect("In memory Storage should not return an error")
            .expect("State transition bytecode version after upgrade should be present")
            .into_owned();

        // Then
        assert_eq!(state_transition_bytecode_version_before_upgrade, 1);
        assert_eq!(state_transition_bytecode_version_after_upgrade, 2);
        assert_eq!(state_transition_bytecode_root_after_upgrade, new_root);
    }

    #[test]
    fn process_upgrade_transaction_should_update_latest_consensus_parameters_version_when_interacting_with_relevant_upgrade(
    ) {
        // Given
        let consensus_parameters = ConsensusParameters::default();
        let serialized_consensus_parameters =
            postcard::to_allocvec(&consensus_parameters)
                .expect("Consensus parameters serialization should succeed");
        let tx_witness = Witness::from(serialized_consensus_parameters.clone());
        let serialized_witness = tx_witness.as_vec();
        let checksum = Hasher::hash(serialized_witness);
        let upgrade_tx =
            TransactionBuilder::upgrade(UpgradePurpose::ConsensusParameters {
                witness_index: 0,
                checksum,
            })
            .add_witness(tx_witness)
            .finalize();

        let mut storage = InMemoryStorage::default();

        // When
        let consensus_parameters_version_before_upgrade = 1;
        let consensus_parameters_version_after_upgrade =
            consensus_parameters_version_before_upgrade + 1;

        let mut storage_tx = storage.write_transaction();
        let mut update_tx = storage_tx
            .construct_update_merkleized_tables_transaction_with_versions(
                0,
                consensus_parameters_version_before_upgrade,
            );

        update_tx.process_upgrade_transaction(&upgrade_tx).unwrap();

        let consensus_parameters_after_upgrade = storage_tx
            .storage_as_ref::<ConsensusParametersVersions>()
            .get(&consensus_parameters_version_after_upgrade)
            .expect("In memory Storage should not return an error")
            .expect("State transition bytecode version after upgrade should be present")
            .into_owned();

        // Then
        assert_eq!(consensus_parameters_version_before_upgrade, 1);
        assert_eq!(consensus_parameters_version_after_upgrade, 2);
        assert_eq!(consensus_parameters_after_upgrade, consensus_parameters);
    }

    #[test]
    /// After processing a transaction,
    /// it should be stored in the `ProcessedTransactions` table.
    fn process_transaction__should_store_processed_transaction() {
        // Given
        let mut storage: InMemoryStorage<Column> = InMemoryStorage::default();
        let mut storage_tx = storage.write_transaction();
        let mut storage_update_tx =
            storage_tx.construct_update_merkleized_tables_transaction();

        let block_height = BlockHeight::new(0);
        let tx_idx = 0;
        let tx = Transaction::default_test_tx();
        let tx_id = tx.id(&storage_update_tx.chain_id);

        // When
        storage_update_tx
            .process_transaction(block_height, tx_idx, &tx)
            .unwrap();

        storage_tx.commit().unwrap();

        // Then
        assert!(storage
            .read_transaction()
            .storage_as_ref::<ProcessedTransactions>()
            .get(&tx_id)
            .unwrap()
            .is_some());
    }

    #[test]
    /// We get an error if we encounter the same transaction
    /// twice in `process_transaction`.
    fn process_transaction__should_error_on_duplicate_transaction() {
        // Given
        let mut storage: InMemoryStorage<Column> = InMemoryStorage::default();
        let mut storage_tx = storage.write_transaction();
        let mut storage_update_tx =
            storage_tx.construct_update_merkleized_tables_transaction();

        let block_height = BlockHeight::new(0);
        let tx_idx = 0;
        let tx = Transaction::default_test_tx();

        // When
        storage_update_tx
            .process_transaction(block_height, tx_idx, &tx)
            .unwrap();

        let result_after_second_call =
            storage_update_tx.process_transaction(block_height, tx_idx, &tx);

        // Then
        assert!(result_after_second_call.is_err());
    }

    #[test]
    /// When encountering a blob transaction,
    /// `process_transaction` should insert the
    /// corresponding blob.
    fn process_transaction__should_insert_blob() {
        let mut rng = StdRng::seed_from_u64(1337);

        // Given
        let blob = vec![1, 3, 3, 7];
        let blob_id = BlobId::compute(&blob);
        let body = BlobBody {
            id: blob_id,
            witness_index: 0,
        };
        let blob_tx = TransactionBuilder::blob(body)
            .add_witness(Witness::from(blob.as_slice()))
            .finalize_as_transaction();

        let mut storage: InMemoryStorage<Column> = InMemoryStorage::default();
        let mut storage_tx = storage.write_transaction();
        let mut storage_update_tx =
            storage_tx.construct_update_merkleized_tables_transaction();

        let block_height = BlockHeight::new(rng.gen());
        let tx_idx = rng.gen();

        // When
        storage_update_tx
            .process_transaction(block_height, tx_idx, &blob_tx)
            .unwrap();

        storage_tx.commit().unwrap();

        let read_tx = storage.read_transaction();
        let blob_in_storage = read_tx
            .storage_as_ref::<Blobs>()
            .get(&blob_id)
            .unwrap()
            .unwrap();

        // Then
        assert_eq!(blob_in_storage.0.as_slice(), blob.as_slice());
    }

    #[test]
    /// When encountering an upload transaction
    /// `process_transaction` should store the
    /// corresponding bytecode segment.
    fn process_transaction__should_upload_bytecode() {
        let mut rng = StdRng::seed_from_u64(1337);

        // Given
        let root = test_helpers::random_bytes(&mut rng);
        let bytecode_segment_1 = vec![4, 2];
        let bytecode_segment_2 = vec![1, 3, 3, 7];

        let upload_body_1 = UploadBody {
            root,
            witness_index: 0,
            subsection_index: 0,
            subsections_number: 2,
            proof_set: Vec::new(),
        };

        let upload_body_2 = UploadBody {
            root,
            witness_index: 0,
            subsection_index: 1,
            subsections_number: 2,
            proof_set: Vec::new(),
        };

        let upload_tx_1 = TransactionBuilder::upload(upload_body_1)
            .add_witness(bytecode_segment_1.clone().into())
            .finalize_as_transaction();

        let upload_tx_2 = TransactionBuilder::upload(upload_body_2)
            .add_witness(bytecode_segment_2.clone().into())
            .finalize_as_transaction();

        let concatenated_bytecode: Vec<_> =
            [bytecode_segment_1.clone(), bytecode_segment_2]
                .into_iter()
                .flatten()
                .collect();

        let mut storage: InMemoryStorage<Column> = InMemoryStorage::default();
        let mut storage_tx = storage.write_transaction();
        let mut storage_update_tx =
            storage_tx.construct_update_merkleized_tables_transaction();

        let block_height = BlockHeight::new(rng.gen());
        let tx_idx = rng.gen();

        // When
        storage_update_tx
            .process_transaction(block_height, tx_idx, &upload_tx_1)
            .unwrap();

        let UploadedBytecode::Uncompleted {
            bytecode: returned_segment_1,
            uploaded_subsections_number: 1,
        } = storage_update_tx
            .storage
            .storage_as_ref::<UploadedBytecodes>()
            .get(&root)
            .unwrap()
            .unwrap()
            .into_owned()
        else {
            panic!("expected incomplete upload")
        };

        storage_update_tx
            .process_transaction(block_height, tx_idx, &upload_tx_2)
            .unwrap();

        storage_tx.commit().unwrap();

        let UploadedBytecode::Completed(returned_bytecode) = storage
            .read_transaction()
            .storage_as_ref::<UploadedBytecodes>()
            .get(&root)
            .unwrap()
            .unwrap()
            .into_owned()
        else {
            panic!("expected complete upload")
        };

        // Then
        assert_eq!(returned_segment_1, bytecode_segment_1);
        assert_eq!(returned_bytecode, concatenated_bytecode);
    }

    #[test]
    fn process_create_transaction__should_insert_bytecode_for_contract_id() {
        // Given
        let contract_bytecode = vec![
            op::addi(0x10, RegId::FP, CallFrame::a_offset().try_into().unwrap()),
            op::lw(0x10, 0x10, 0),
            op::addi(0x11, RegId::FP, CallFrame::b_offset().try_into().unwrap()),
            op::lw(0x11, 0x11, 0),
            op::addi(0x12, 0x11, 32),
            op::addi(0x13, RegId::ZERO, 0),
            op::tro(0x12, 0x13, 0x10, 0x11),
            op::ret(RegId::ONE),
        ]
        .into_iter()
        .collect::<Vec<u8>>();

        let mut rng = StdRng::seed_from_u64(1337);
        let create_contract_tx =
            test_helpers::create_contract_tx(&contract_bytecode, &mut rng);
        let contract_id = create_contract_tx
            .metadata()
            .as_ref()
            .unwrap()
            .body
            .contract_id;

        let mut storage: InMemoryStorage<Column> = InMemoryStorage::default();
        let mut storage_tx = storage.write_transaction();
        let mut storage_update_tx =
            storage_tx.construct_update_merkleized_tables_transaction();

        // When
        storage_update_tx
            .process_create_transaction(&create_contract_tx)
            .unwrap();

        storage_tx.commit().unwrap();
        let stored_contract = storage
            .read_transaction()
            .storage_as_ref::<ContractsRawCode>()
            .get(&contract_id)
            .unwrap()
            .unwrap()
            .into_owned();
        // Then
        assert_eq!(stored_contract, Contract::from(contract_bytecode));
    }

    trait ConstructUpdateMerkleizedTablesTransactionForTests<'a>: Sized + 'a {
        type Storage;
        fn construct_update_merkleized_tables_transaction(
            self,
        ) -> UpdateMerkleizedTablesTransaction<'a, Self::Storage>;

        fn construct_update_merkleized_tables_transaction_with_versions(
            self,
            latest_state_transition_bytecode_version: StateTransitionBytecodeVersion,
            latest_consensus_parameters_version: ConsensusParametersVersion,
        ) -> UpdateMerkleizedTablesTransaction<'a, Self::Storage>;
    }

    impl<'a, Storage> ConstructUpdateMerkleizedTablesTransactionForTests<'a>
        for &'a mut StorageTransaction<Storage>
    {
        type Storage = Storage;

        fn construct_update_merkleized_tables_transaction(
            self,
        ) -> UpdateMerkleizedTablesTransaction<'a, Self::Storage> {
            UpdateMerkleizedTablesTransaction {
                chain_id: ChainId::default(),
                storage: self,
                latest_consensus_parameters_version: Default::default(),
                latest_state_transition_bytecode_version: Default::default(),
            }
        }
        fn construct_update_merkleized_tables_transaction_with_versions(
            self,
            latest_state_transition_bytecode_version: StateTransitionBytecodeVersion,
            latest_consensus_parameters_version: ConsensusParametersVersion,
        ) -> UpdateMerkleizedTablesTransaction<'a, Self::Storage> {
            UpdateMerkleizedTablesTransaction {
                chain_id: ChainId::default(),
                storage: self,
                latest_consensus_parameters_version,
                latest_state_transition_bytecode_version,
            }
        }
    }
}
