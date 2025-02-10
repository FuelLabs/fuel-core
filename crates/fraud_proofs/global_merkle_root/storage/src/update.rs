use crate::{
    column::Column,
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
use anyhow::anyhow;
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
    services::executor::{
        Error as ExecutorError,
        TransactionValidityError,
    },
};

pub trait UpdateMerkleizedTables {
    fn update_merkleized_tables(
        &mut self,
        chain_id: ChainId,
        block: &Block,
    ) -> anyhow::Result<&mut Self>;
}

impl<Storage> UpdateMerkleizedTables for StorageTransaction<Storage>
where
    Storage: KeyValueInspect<Column = Column>,
{
    fn update_merkleized_tables(
        &mut self,
        chain_id: ChainId,
        block: &Block,
    ) -> anyhow::Result<&mut Self> {
        let mut update_transaction = UpdateMerkleizedTablesTransaction {
            chain_id,
            storage: self,
            latest_state_transition_bytecode_version: block
                .header()
                .state_transition_bytecode_version,
            latest_consensus_parameters_version: block
                .header()
                .consensus_parameters_version,
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
    // TODO(#2588): Proper result type
    pub fn process_block(&mut self, block: &Block) -> anyhow::Result<()> {
        let block_height = *block.header().height();

        for (tx_idx, tx) in block.transactions().iter().enumerate() {
            let tx_idx: u16 =
                u16::try_from(tx_idx).map_err(|_| ExecutorError::TooManyTransactions)?;
            self.process_transaction(block_height, tx_idx, tx)?;
        }

        Ok(())
    }

    fn process_transaction(
        &mut self,
        block_height: BlockHeight,
        tx_idx: u16,
        tx: &Transaction,
    ) -> anyhow::Result<()> {
        let tx_id = tx.id(&self.chain_id);
        let inputs = tx.inputs();

        for input in inputs.iter() {
            self.process_input(input)?;
        }

        let tx_pointer = TxPointer::new(block_height, tx_idx);
        for (output_index, output) in tx.outputs().iter().enumerate() {
            let output_index =
                u16::try_from(output_index).map_err(|_| ExecutorError::TooManyOutputs)?;

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

    fn process_input(&mut self, input: &Input) -> anyhow::Result<()> {
        match input {
            Input::CoinSigned(CoinSigned { utxo_id, .. })
            | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                self.storage.storage_as_mut::<Coins>().remove(utxo_id)?;
            }
            Input::Contract(_) => {
                // Do nothing, since we are interested in output values
            }
            Input::MessageCoinSigned(MessageCoinSigned { nonce, .. })
            | Input::MessageCoinPredicate(MessageCoinPredicate { nonce, .. }) => {
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
                    self.storage.storage_as_mut::<Messages>().remove(nonce)?;
                }
            }
        }

        Ok(())
    }

    fn process_output(
        &mut self,
        tx_pointer: TxPointer,
        utxo_id: UtxoId,
        inputs: &[Input],
        output: &Output,
    ) -> anyhow::Result<()> {
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
                self.storage.storage::<ContractsLatestUtxo>().insert(
                    contract_id,
                    &ContractUtxoInfo::V1((utxo_id, tx_pointer).into()),
                )?;
            }
        }
        Ok(())
    }

    fn store_processed_transaction(&mut self, tx_id: TxId) -> anyhow::Result<()> {
        let previous_tx = self
            .storage
            .storage_as_mut::<ProcessedTransactions>()
            .replace(&tx_id, &())?;

        if previous_tx.is_some() {
            anyhow::bail!("duplicate transaction detected")
        };

        Ok(())
    }

    fn insert_coin_if_it_has_amount(
        &mut self,
        tx_pointer: TxPointer,
        utxo_id: UtxoId,
        owner: Address,
        amount: Word,
        asset_id: AssetId,
    ) -> anyhow::Result<()> {
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

            let previous_coin =
                self.storage.storage::<Coins>().replace(&utxo_id, &coin)?;

            // We should never overwrite coins.
            // TODO(#2588): Return error instead.
            assert!(previous_coin.is_none());
        }

        Ok(())
    }

    fn try_insert_latest_contract_utxo(
        &mut self,
        tx_pointer: TxPointer,
        utxo_id: UtxoId,
        inputs: &[Input],
        contract: output::contract::Contract,
    ) -> anyhow::Result<()> {
        if let Some(Input::Contract(input::contract::Contract { contract_id, .. })) =
            inputs.get(contract.input_index as usize)
        {
            self.storage.storage::<ContractsLatestUtxo>().insert(
                contract_id,
                &ContractUtxoInfo::V1((utxo_id, tx_pointer).into()),
            )?;
        } else {
            Err(ExecutorError::TransactionValidity(
                TransactionValidityError::InvalidContractInputIndex(utxo_id),
            ))?;
        }
        Ok(())
    }

    fn process_create_transaction(&mut self, tx: &Create) -> anyhow::Result<()> {
        let bytecode_witness_index = tx.bytecode_witness_index();
        let witnesses = tx.witnesses();
        let bytecode = witnesses
            .get(usize::from(*bytecode_witness_index))
            .ok_or_else(|| anyhow!("invalid witness index {bytecode_witness_index}"))?
            .as_vec();

        // The Fuel specs mandate that each create transaction has exactly one output of type `Output::ContractCreated`.
        // See https://docs.fuel.network/docs/specs/tx-format/transaction/#transactioncreate
        let Some(Output::ContractCreated { contract_id, .. }) = tx
            .outputs()
            .iter()
            .find(|output| matches!(output, Output::ContractCreated { .. }))
        else {
            anyhow::bail!("Create transaction does not have contract created output")
        };

        self.storage
            .storage_as_mut::<ContractsRawCode>()
            .insert(contract_id, bytecode)?;
        Ok(())
    }

    fn process_upgrade_transaction(&mut self, tx: &Upgrade) -> anyhow::Result<()> {
        let metadata = match tx.metadata() {
            Some(metadata) => metadata.body.clone(),
            None => UpgradeMetadata::compute(tx).map_err(|e| anyhow::anyhow!(e))?,
        };

        match metadata {
            UpgradeMetadata::ConsensusParameters {
                consensus_parameters,
                calculated_checksum: _,
            } => {
                let Some(next_consensus_parameters_version) =
                    self.latest_consensus_parameters_version.checked_add(1)
                else {
                    return Err(anyhow::anyhow!("Invalid consensus parameters version"));
                };
                self.latest_consensus_parameters_version =
                    next_consensus_parameters_version;
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
                        return Err(anyhow::anyhow!(
                            "Invalid state transition bytecode version"
                        ));
                    };
                    self.latest_state_transition_bytecode_version =
                        next_state_transition_bytecode_version;
                    self.storage
                        .storage::<StateTransitionBytecodeVersions>()
                        .insert(&self.latest_state_transition_bytecode_version, root)?;
                }
            },
        }

        Ok(())
    }

    fn process_upload_transaction(&mut self, tx: &Upload) -> anyhow::Result<()> {
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
            anyhow::bail!("expected uncompleted bytecode")
        };

        let expected_subsection_index = uploaded_subsections_number;
        if expected_subsection_index != tx.body().subsection_index {
            anyhow::bail!("expected subsection index {expected_subsection_index}");
        };

        let witness_index = usize::from(*tx.bytecode_witness_index());
        let new_bytecode = tx
            .witnesses()
            .get(witness_index)
            .ok_or_else(|| anyhow!("expected witness with bytecode"))?;

        bytecode.extend_from_slice(new_bytecode.as_ref());

        let new_uploaded_subsections_number =
            uploaded_subsections_number.checked_add(1).ok_or_else(|| {
                anyhow!("overflow when incrementing uploaded subsection number")
            })?;

        let new_uploaded_bytecode =
            if new_uploaded_subsections_number == tx.body().subsections_number {
                UploadedBytecode::Completed(bytecode)
            } else {
                UploadedBytecode::Uncompleted {
                    bytecode,
                    uploaded_subsections_number: new_uploaded_subsections_number,
                }
            };

        self.storage
            .storage_as_mut::<UploadedBytecodes>()
            .insert(&bytecode_root, &new_uploaded_bytecode)?;

        Ok(())
    }

    fn process_blob_transaction(&mut self, tx: &Blob) -> anyhow::Result<()> {
        let BlobBody {
            id: blob_id,
            witness_index,
        } = tx.body();

        let blob = tx
            .witnesses()
            .get(usize::from(*witness_index))
             // TODO(#2588): Proper error type
            .ok_or_else(|| anyhow!("transaction should have blob payload"))?;

        self.storage
            .storage::<Blobs>()
            .insert(blob_id, blob.as_ref())?;

        Ok(())
    }
}

pub trait TransactionInputs {
    fn inputs(&self) -> Cow<Vec<Input>>;
}

pub trait TransactionOutputs {
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
            ContractId,
            Create,
            Finalizable,
            TransactionBuilder,
            TxId,
            UploadBody,
            Witness,
        },
        fuel_vm::{
            CallFrame,
            Salt,
        },
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

        let tx_pointer = random_tx_pointer(&mut rng);
        let utxo_id = random_utxo_id(&mut rng);
        let inputs = vec![];

        let output_amount = rng.gen();
        let output_address = random_address(&mut rng);
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
    /// populated in the `ContractCreated` table.
    fn process_output__should_insert_latest_contract_utxo_when_contract_created() {
        let mut rng = StdRng::seed_from_u64(1337);

        // Given
        let mut storage: InMemoryStorage<Column> = InMemoryStorage::default();
        let mut storage_tx = storage.write_transaction();
        let mut storage_update_tx =
            storage_tx.construct_update_merkleized_tables_transaction();

        let tx_pointer = random_tx_pointer(&mut rng);
        let utxo_id = random_utxo_id(&mut rng);
        let inputs = vec![];

        let contract_id = random_contract_id(&mut rng);
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
    /// populated in the `ContractCreated` table.
    fn process_output__should_update_latest_contract_utxo_when_interacting_with_contract()
    {
        let mut rng = StdRng::seed_from_u64(1337);

        // Given
        let mut storage: InMemoryStorage<Column> = InMemoryStorage::default();
        let mut storage_tx = storage.write_transaction();
        let mut storage_update_tx =
            storage_tx.construct_update_merkleized_tables_transaction();

        let tx_pointer = random_tx_pointer(&mut rng);
        let utxo_id = random_utxo_id(&mut rng);

        let contract_id = random_contract_id(&mut rng);
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
        let output_address = random_address(&mut rng);
        let tx_pointer = random_tx_pointer(&mut rng);
        let utxo_id = random_utxo_id(&mut rng);
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
        let root = random_bytes(&mut rng);
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
        let create_contract_tx = create_contract_tx(&contract_bytecode, &mut rng);
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

    // TODO: https://github.com/FuelLabs/fuel-core/issues/2654
    // This code is copied from the executor. We should refactor it to be shared.
    fn create_contract_tx(bytecode: &[u8], rng: &mut impl rand::RngCore) -> Create {
        let salt: Salt = rng.gen();
        let contract = Contract::from(bytecode);
        let root = contract.root();
        let state_root = Contract::default_state_root();
        let contract_id = contract.id(&salt, &root, &state_root);

        TransactionBuilder::create(bytecode.into(), salt, Default::default())
            .add_fee_input()
            .add_output(Output::contract_created(contract_id, state_root))
            .finalize()
    }

    fn random_utxo_id(rng: &mut impl rand::RngCore) -> UtxoId {
        let mut txid = TxId::default();
        rng.fill_bytes(txid.as_mut());
        let output_index = rng.gen();

        UtxoId::new(txid, output_index)
    }

    fn random_tx_pointer(rng: &mut impl rand::RngCore) -> TxPointer {
        let block_height = BlockHeight::new(rng.gen());
        let tx_index = rng.gen();

        TxPointer::new(block_height, tx_index)
    }

    fn random_address(rng: &mut impl rand::RngCore) -> Address {
        let mut address = Address::default();
        rng.fill_bytes(address.as_mut());

        address
    }

    fn random_contract_id(rng: &mut impl rand::RngCore) -> ContractId {
        let mut contract_id = ContractId::default();
        rng.fill_bytes(contract_id.as_mut());

        contract_id
    }

    fn random_bytes(rng: &mut impl rand::RngCore) -> Bytes32 {
        let mut bytes = Bytes32::default();
        rng.fill_bytes(bytes.as_mut());

        bytes
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
