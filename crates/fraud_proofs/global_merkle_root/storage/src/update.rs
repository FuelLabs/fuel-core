use crate::{
    column::Column,
    Coins,
    ContractsLatestUtxo,
    Messages,
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
        output::{
            self,
        },
        Address,
        AssetId,
        Input,
        Output,
        Transaction,
        TxPointer,
        UniqueIdentifier,
        UtxoId,
    },
    fuel_types::{
        BlockHeight,
        ChainId,
    },
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
        };

        update_transaction.process_block(block)?;

        Ok(self)
    }
}

struct UpdateMerkleizedTablesTransaction<'a, Storage> {
    chain_id: ChainId,
    storage: &'a mut StorageTransaction<Storage>,
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
        let inputs = tx.inputs();
        for input in inputs.iter() {
            self.process_input(input)?;
        }

        let tx_pointer = TxPointer::new(block_height, tx_idx);
        for (output_index, output) in tx.outputs().iter().enumerate() {
            let output_index =
                u16::try_from(output_index).map_err(|_| ExecutorError::TooManyOutputs)?;

            let tx_id = tx.id(&self.chain_id);
            let utxo_id = UtxoId::new(tx_id, output_index);
            self.process_output(tx_pointer, utxo_id, &inputs, output)?;
        }

        // TODO(#2583): Add the transaction to the `ProcessedTransactions` table.
        // TODO(#2584): Insert state transition bytecode and consensus parameter updates.
        // TODO(#2585): Insert uplodade bytecodes.
        // TODO(#2586): Insert blobs.
        // TODO(#2587): Insert raw code for created contracts.

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
    use fuel_core_types::fuel_tx::{
        Bytes32,
        ContractId,
        TxId,
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

    trait ConstructUpdateMerkleizedTablesTransactionForTests<'a>: Sized + 'a {
        type Storage;
        fn construct_update_merkleized_tables_transaction(
            self,
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
            }
        }
    }
}
