#[allow(clippy::arithmetic_side_effects)]
#[allow(clippy::cast_possible_truncation)]
#[cfg(test)]
mod tests {
    use crate::database::Database;
    use fuel_core_executor::{
        executor::{
            block_component::PartialBlockComponent,
            ExecutionData,
            ExecutionOptions,
            Executor,
            OnceTransactionsSource,
        },
        ports::RelayerPort,
        refs::ContractRef,
        Config,
    };
    use fuel_core_storage::{
        tables::{
            Coins,
            ContractsRawCode,
            Messages,
        },
        transactional::AtomicView,
        Result as StorageResult,
        StorageAsMut,
    };
    use fuel_core_types::{
        blockchain::{
            block::{
                Block,
                PartialFuelBlock,
            },
            header::{
                ConsensusHeader,
                PartialBlockHeader,
            },
            primitives::DaBlockHeight,
        },
        entities::{
            coins::coin::CompressedCoin,
            message::{
                Message,
                MessageV1,
            },
        },
        fuel_asm::{
            op,
            GTFArgs,
            RegId,
        },
        fuel_crypto::SecretKey,
        fuel_merkle::sparse,
        fuel_tx::{
            field::{
                InputContract,
                Inputs,
                MintAmount,
                MintAssetId,
                OutputContract,
                Outputs,
                Script as ScriptField,
                TxPointer as TxPointerTraitTrait,
            },
            input::{
                coin::CoinSigned,
                contract,
                Input,
            },
            Bytes32,
            Cacheable,
            Chargeable,
            ConsensusParameters,
            Create,
            FeeParameters,
            Finalizable,
            Output,
            Receipt,
            Script,
            Transaction,
            TransactionBuilder,
            TransactionFee,
            TxParameters,
            TxPointer,
            UniqueIdentifier,
            UtxoId,
            ValidityError,
        },
        fuel_types::{
            canonical::Serialize,
            Address,
            AssetId,
            BlockHeight,
            ChainId,
            ContractId,
            Salt,
            Word,
        },
        fuel_vm::{
            checked_transaction::CheckError,
            interpreter::ExecutableTransaction,
            script_with_data_offset,
            util::test_helpers::TestBuilder as TxBuilder,
            Call,
            CallFrame,
            Contract,
        },
        services::{
            block_producer::Components,
            executor::{
                Error as ExecutorError,
                Event as ExecutorEvent,
                ExecutionBlock,
                ExecutionResult,
                ExecutionType,
                ExecutionTypes,
                TransactionExecutionResult,
                TransactionValidityError,
            },
            relayer::Event,
        },
        tai64::Tai64,
    };
    use itertools::Itertools;
    use rand::{
        prelude::StdRng,
        Rng,
        SeedableRng,
    };
    use std::{
        ops::DerefMut,
        sync::Arc,
    };

    #[derive(Clone, Debug)]
    struct DisabledRelayer;

    impl RelayerPort for DisabledRelayer {
        fn enabled(&self) -> bool {
            false
        }

        fn get_events(&self, _: &DaBlockHeight) -> anyhow::Result<Vec<Event>> {
            unimplemented!()
        }
    }

    impl AtomicView for DisabledRelayer {
        type View = Self;
        type Height = DaBlockHeight;

        fn latest_height(&self) -> Self::Height {
            0u64.into()
        }

        fn view_at(&self, _: &Self::Height) -> StorageResult<Self::View> {
            Ok(self.latest_view())
        }

        fn latest_view(&self) -> Self::View {
            self.clone()
        }
    }

    fn create_executor(
        database: Database,
        config: Config,
    ) -> Executor<Database, DisabledRelayer> {
        Executor {
            database_view_provider: database,
            relayer_view_provider: DisabledRelayer,
            config: Arc::new(config),
        }
    }

    pub(crate) fn setup_executable_script() -> (Create, Script) {
        let mut rng = StdRng::seed_from_u64(2322);
        let asset_id: AssetId = rng.gen();
        let owner: Address = rng.gen();
        let input_amount = 1000;
        let variable_transfer_amount = 100;
        let coin_output_amount = 150;

        let (create, contract_id) = create_contract(
            vec![
                // load amount of coins to 0x10
                op::addi(0x10, RegId::FP, CallFrame::a_offset().try_into().unwrap()),
                op::lw(0x10, 0x10, 0),
                // load asset id to 0x11
                op::addi(0x11, RegId::FP, CallFrame::b_offset().try_into().unwrap()),
                op::lw(0x11, 0x11, 0),
                // load address to 0x12
                op::addi(0x12, 0x11, 32),
                // load output index (0) to 0x13
                op::addi(0x13, RegId::ZERO, 0),
                op::tro(0x12, 0x13, 0x10, 0x11),
                op::ret(RegId::ONE),
            ]
            .into_iter()
            .collect::<Vec<u8>>(),
            &mut rng,
        );
        let (script, data_offset) = script_with_data_offset!(
            data_offset,
            vec![
                // set reg 0x10 to call data
                op::movi(0x10, data_offset + 64),
                // set reg 0x11 to asset id
                op::movi(0x11, data_offset),
                // set reg 0x12 to call amount
                op::movi(0x12, variable_transfer_amount),
                // call contract without any tokens to transfer in (3rd arg arbitrary when 2nd is zero)
                op::call(0x10, 0x12, 0x11, RegId::CGAS),
                op::ret(RegId::ONE),
            ],
            TxParameters::DEFAULT.tx_offset()
        );

        let script_data: Vec<u8> = [
            asset_id.as_ref(),
            owner.as_ref(),
            Call::new(
                contract_id,
                variable_transfer_amount as Word,
                data_offset as Word,
            )
            .to_bytes()
            .as_ref(),
        ]
        .into_iter()
        .flatten()
        .copied()
        .collect();

        let script = TxBuilder::new(2322)
            .script_gas_limit(TxParameters::DEFAULT.max_gas_per_tx >> 1)
            .start_script(script, script_data)
            .contract_input(contract_id)
            .coin_input(asset_id, input_amount)
            .variable_output(Default::default())
            .coin_output(asset_id, coin_output_amount)
            .change_output(asset_id)
            .contract_output(&contract_id)
            .build()
            .transaction()
            .clone();

        (create, script)
    }

    pub(crate) fn test_block(
        block_height: BlockHeight,
        da_block_height: DaBlockHeight,
        num_txs: usize,
    ) -> Block {
        let transactions = (1..num_txs + 1)
            .map(|i| {
                TxBuilder::new(2322u64)
                    .script_gas_limit(10)
                    .coin_input(AssetId::default(), (i as Word) * 100)
                    .coin_output(AssetId::default(), (i as Word) * 50)
                    .change_output(AssetId::default())
                    .build()
                    .transaction()
                    .clone()
                    .into()
            })
            .collect_vec();

        let mut block = Block::default();
        block.header_mut().set_block_height(block_height);
        block.header_mut().set_da_height(da_block_height);
        *block.transactions_mut() = transactions;
        block
    }

    pub(crate) fn create_contract<R: Rng>(
        contract_code: Vec<u8>,
        rng: &mut R,
    ) -> (Create, ContractId) {
        let salt: Salt = rng.gen();
        let contract = Contract::from(contract_code.clone());
        let root = contract.root();
        let state_root = Contract::default_state_root();
        let contract_id = contract.id(&salt, &root, &state_root);

        let tx =
            TransactionBuilder::create(contract_code.into(), salt, Default::default())
                .add_random_fee_input()
                .add_output(Output::contract_created(contract_id, state_root))
                .finalize();
        (tx, contract_id)
    }

    // Happy path test case that a produced block will also validate
    #[test]
    fn executor_validates_correctly_produced_block() {
        let producer = create_executor(Default::default(), Default::default());
        let verifier = create_executor(Default::default(), Default::default());
        let block = test_block(1u32.into(), 0u64.into(), 10);

        let ExecutionResult {
            block,
            skipped_transactions,
            ..
        } = producer
            .execute_and_commit(
                ExecutionTypes::Production(block.into()),
                Default::default(),
            )
            .unwrap();

        let validation_result = verifier
            .execute_and_commit(ExecutionTypes::Validation(block), Default::default());
        assert!(validation_result.is_ok());
        assert!(skipped_transactions.is_empty());
    }

    // Ensure transaction commitment != default after execution
    #[test]
    fn executor_commits_transactions_to_block() {
        let producer = create_executor(Default::default(), Default::default());
        let block = test_block(1u32.into(), 0u64.into(), 10);
        let start_block = block.clone();

        let ExecutionResult {
            block,
            skipped_transactions,
            ..
        } = producer
            .execute_and_commit(
                ExecutionBlock::Production(block.into()),
                Default::default(),
            )
            .unwrap();

        assert!(skipped_transactions.is_empty());
        assert_ne!(
            start_block.header().transactions_root,
            block.header().transactions_root
        );
        assert_eq!(block.transactions().len(), 11);
        assert!(block.transactions()[10].as_mint().is_some());
        if let Some(mint) = block.transactions()[10].as_mint() {
            assert_eq!(
                mint.tx_pointer(),
                &TxPointer::new(*block.header().height(), 10)
            );
            assert_eq!(mint.mint_asset_id(), &AssetId::BASE);
            assert_eq!(mint.mint_amount(), &0);
            assert_eq!(mint.input_contract().contract_id, ContractId::zeroed());
            assert_eq!(mint.input_contract().balance_root, Bytes32::zeroed());
            assert_eq!(mint.input_contract().state_root, Bytes32::zeroed());
            assert_eq!(mint.input_contract().utxo_id, UtxoId::default());
            assert_eq!(mint.input_contract().tx_pointer, TxPointer::default());
            assert_eq!(mint.output_contract().balance_root, Bytes32::zeroed());
            assert_eq!(mint.output_contract().state_root, Bytes32::zeroed());
            assert_eq!(mint.output_contract().input_index, 0);
        } else {
            panic!("Invalid outputs of coinbase");
        }
    }

    mod coinbase {
        use super::*;
        use fuel_core_storage::transactional::AtomicView;

        #[test]
        fn executor_commits_transactions_with_non_zero_coinbase_generation() {
            // The test verifies the correctness of the coinbase contract update.
            // The test generates two blocks with a non-zero fee.
            //
            // The first block contains one valid and one invalid transaction.
            // This part of the test verifies that the invalid transaction doesn't influence
            // the final fee, and the final is the same as the `max_fee` of the valid transaction.
            //
            // The second block contains only a valid transaction, and it uses
            // the `Mint` transaction from the first block to validate the contract
            // state transition between blocks.
            let price = 1;
            let limit = 0;
            let gas_price_factor = 1;
            let script = TxBuilder::new(1u64)
                .script_gas_limit(limit)
                // Set a price for the test
                .gas_price(price)
                .coin_input(AssetId::BASE, 10000)
                .change_output(AssetId::BASE)
                .build()
                .transaction()
                .clone();

            let recipient = Contract::EMPTY_CONTRACT_ID;

            let fee_params = FeeParameters {
                gas_price_factor,
                ..Default::default()
            };
            let config = Config {
                coinbase_recipient: recipient,
                consensus_parameters: ConsensusParameters {
                    fee_params,
                    ..Default::default()
                },
                ..Default::default()
            };

            let database = &mut Database::default();
            database
                .storage::<ContractsRawCode>()
                .insert(&recipient, &[])
                .expect("Should insert coinbase contract");

            let producer = create_executor(database.clone(), config);

            let expected_fee_amount_1 = TransactionFee::checked_from_tx(
                producer.config.consensus_parameters.gas_costs(),
                producer.config.consensus_parameters.fee_params(),
                &script,
            )
            .unwrap()
            .max_fee();
            let invalid_duplicate_tx = script.clone().into();

            let mut block = Block::default();
            block.header_mut().set_block_height(1.into());
            *block.transactions_mut() = vec![script.into(), invalid_duplicate_tx];
            block.header_mut().recalculate_metadata();

            let ExecutionResult {
                block,
                skipped_transactions,
                ..
            } = producer
                .execute_and_commit(
                    ExecutionBlock::Production(block.into()),
                    Default::default(),
                )
                .unwrap();

            assert_eq!(skipped_transactions.len(), 1);
            assert_eq!(block.transactions().len(), 2);
            assert!(expected_fee_amount_1 > 0);
            let first_mint;

            if let Some(mint) = block.transactions()[1].as_mint() {
                assert_eq!(
                    mint.tx_pointer(),
                    &TxPointer::new(*block.header().height(), 1)
                );
                assert_eq!(mint.mint_asset_id(), &AssetId::BASE);
                assert_eq!(mint.mint_amount(), &expected_fee_amount_1);
                assert_eq!(mint.input_contract().contract_id, recipient);
                assert_eq!(mint.input_contract().balance_root, Bytes32::zeroed());
                assert_eq!(mint.input_contract().state_root, Bytes32::zeroed());
                assert_eq!(mint.input_contract().utxo_id, UtxoId::default());
                assert_eq!(mint.input_contract().tx_pointer, TxPointer::default());
                assert_ne!(mint.output_contract().balance_root, Bytes32::zeroed());
                assert_eq!(mint.output_contract().state_root, Bytes32::zeroed());
                assert_eq!(mint.output_contract().input_index, 0);
                first_mint = mint.clone();
            } else {
                panic!("Invalid coinbase transaction");
            }

            let (asset_id, amount) = producer
                .database_view_provider
                .latest_view()
                .contract_balances(recipient, None, None)
                .next()
                .unwrap()
                .unwrap();
            assert_eq!(asset_id, AssetId::zeroed());
            assert_eq!(amount, expected_fee_amount_1);

            let script = TxBuilder::new(2u64)
                .script_gas_limit(limit)
                // Set a price for the test
                .gas_price(price)
                .coin_input(AssetId::BASE, 10000)
                .change_output(AssetId::BASE)
                .build()
                .transaction()
                .clone();

            let expected_fee_amount_2 = TransactionFee::checked_from_tx(
                producer.config.consensus_parameters.gas_costs(),
                producer.config.consensus_parameters.fee_params(),
                &script,
            )
            .unwrap()
            .max_fee();

            let mut block = Block::default();
            block.header_mut().set_block_height(2.into());
            *block.transactions_mut() = vec![script.into()];
            block.header_mut().recalculate_metadata();

            let ExecutionResult {
                block,
                skipped_transactions,
                ..
            } = producer
                .execute_and_commit(
                    ExecutionBlock::Production(block.into()),
                    Default::default(),
                )
                .unwrap();

            assert_eq!(skipped_transactions.len(), 0);
            assert_eq!(block.transactions().len(), 2);

            if let Some(second_mint) = block.transactions()[1].as_mint() {
                assert_eq!(second_mint.tx_pointer(), &TxPointer::new(2.into(), 1));
                assert_eq!(second_mint.mint_asset_id(), &AssetId::BASE);
                assert_eq!(second_mint.mint_amount(), &expected_fee_amount_2);
                assert_eq!(second_mint.input_contract().contract_id, recipient);
                assert_eq!(
                    second_mint.input_contract().balance_root,
                    first_mint.output_contract().balance_root
                );
                assert_eq!(
                    second_mint.input_contract().state_root,
                    first_mint.output_contract().state_root
                );
                assert_eq!(
                    second_mint.input_contract().utxo_id,
                    UtxoId::new(first_mint.cached_id().expect("Id exists"), 0)
                );
                assert_eq!(
                    second_mint.input_contract().tx_pointer,
                    TxPointer::new(1.into(), 1)
                );
                assert_ne!(
                    second_mint.output_contract().balance_root,
                    first_mint.output_contract().balance_root
                );
                assert_eq!(
                    second_mint.output_contract().state_root,
                    first_mint.output_contract().state_root
                );
                assert_eq!(second_mint.output_contract().input_index, 0);
            } else {
                panic!("Invalid coinbase transaction");
            }
            let (asset_id, amount) = producer
                .database_view_provider
                .latest_view()
                .contract_balances(recipient, None, None)
                .next()
                .unwrap()
                .unwrap();
            assert_eq!(asset_id, AssetId::zeroed());
            assert_eq!(amount, expected_fee_amount_1 + expected_fee_amount_2);
        }

        #[test]
        fn skip_coinbase_during_dry_run() {
            let price = 1;
            let limit = 0;
            let gas_price_factor = 1;
            let script = TxBuilder::new(2322u64)
                .script_gas_limit(limit)
                // Set a price for the test
                .gas_price(price)
                .coin_input(AssetId::BASE, 10000)
                .change_output(AssetId::BASE)
                .build()
                .transaction()
                .clone();

            let mut config = Config::default();
            let recipient = [1u8; 32].into();
            config.coinbase_recipient = recipient;

            config.consensus_parameters.fee_params.gas_price_factor = gas_price_factor;

            let producer = create_executor(Default::default(), config);

            let result = producer
                .execute_without_commit(ExecutionTypes::DryRun(Components {
                    header_to_produce: Default::default(),
                    transactions_source: OnceTransactionsSource::new(vec![script.into()]),
                    gas_limit: u64::MAX,
                }))
                .unwrap();
            let ExecutionResult { block, .. } = result.into_result();

            assert_eq!(block.transactions().len(), 1);
        }

        #[test]
        fn executor_commits_transactions_with_non_zero_coinbase_validation() {
            let price = 1;
            let limit = 0;
            let gas_price_factor = 1;
            let script = TxBuilder::new(2322u64)
                .script_gas_limit(limit)
                // Set a price for the test
                .gas_price(price)
                .coin_input(AssetId::BASE, 10000)
                .change_output(AssetId::BASE)
                .build()
                .transaction()
                .clone();
            let recipient = Contract::EMPTY_CONTRACT_ID;

            let fee_params = FeeParameters {
                gas_price_factor,
                ..Default::default()
            };
            let config = Config {
                coinbase_recipient: recipient,
                consensus_parameters: ConsensusParameters {
                    fee_params,
                    ..Default::default()
                },
                ..Default::default()
            };
            let database = &mut Database::default();

            database
                .storage::<ContractsRawCode>()
                .insert(&recipient, &[])
                .expect("Should insert coinbase contract");

            let producer = create_executor(database.clone(), config);

            let mut block = Block::default();
            *block.transactions_mut() = vec![script.into()];

            let ExecutionResult {
                block: produced_block,
                skipped_transactions,
                ..
            } = producer
                .execute_and_commit(
                    ExecutionBlock::Production(block.into()),
                    Default::default(),
                )
                .unwrap();
            assert!(skipped_transactions.is_empty());
            let produced_txs = produced_block.transactions().to_vec();

            let validator = create_executor(
                Default::default(),
                // Use the same config as block producer
                producer.config.as_ref().clone(),
            );
            let ExecutionResult {
                block: validated_block,
                ..
            } = validator
                .execute_and_commit(
                    ExecutionBlock::Validation(produced_block),
                    Default::default(),
                )
                .unwrap();
            assert_eq!(validated_block.transactions(), produced_txs);
            let (asset_id, amount) = validator
                .database_view_provider
                .latest_view()
                .contract_balances(recipient, None, None)
                .next()
                .unwrap()
                .unwrap();
            assert_eq!(asset_id, AssetId::zeroed());
            assert_ne!(amount, 0);
        }

        #[test]
        fn execute_cb_command() {
            fn compare_coinbase_addresses(
                config_coinbase: ContractId,
                expected_in_tx_coinbase: ContractId,
            ) -> bool {
                let script = TxBuilder::new(2322u64)
                    .script_gas_limit(100000)
                    // Set a price for the test
                    .gas_price(0)
                    .start_script(vec![
                        // Store the size of the `Address`(32 bytes) into register `0x11`.
                        op::movi(0x11, Address::LEN.try_into().unwrap()),
                        // Allocate 32 bytes on the heap.
                        op::aloc(0x11),
                        // Store the pointer to the beginning of the free memory into
                        // register `0x10`.
                        op::move_(0x10, RegId::HP),
                        // Store `config_coinbase` `Address` into MEM[$0x10; 32].
                        op::cb(0x10),
                        // Store the pointer on the beginning of script data into register `0x12`.
                        // Script data contains `expected_in_tx_coinbase` - 32 bytes of data.
                        op::gtf_args(0x12, 0x00, GTFArgs::ScriptData),
                        // Compare retrieved `config_coinbase`(register `0x10`) with
                        // passed `expected_in_tx_coinbase`(register `0x12`) where the length
                        // of memory comparison is 32 bytes(register `0x11`) and store result into
                        // register `0x13`(1 - true, 0 - false).
                        op::meq(0x13, 0x10, 0x12, 0x11),
                        // Return the result of the comparison as a receipt.
                        op::ret(0x13)
                    ], expected_in_tx_coinbase.to_vec() /* pass expected address as script data */)
                    .coin_input(AssetId::BASE, 1000)
                    .variable_output(Default::default())
                    .coin_output(AssetId::BASE, 1000)
                    .change_output(AssetId::BASE)
                    .build()
                    .transaction()
                    .clone();

                let config = Config {
                    coinbase_recipient: config_coinbase,
                    ..Default::default()
                };
                let producer = create_executor(Default::default(), config);

                let mut block = Block::default();
                *block.transactions_mut() = vec![script.clone().into()];

                let ExecutionResult { tx_status, .. } = producer
                    .execute_and_commit(
                        ExecutionBlock::Production(block.into()),
                        Default::default(),
                    )
                    .expect("Should execute the block");
                let receipts = tx_status[0].result.receipts();

                if let Some(Receipt::Return { val, .. }) = receipts.first() {
                    *val == 1
                } else {
                    panic!("Execution of the `CB` script failed failed")
                }
            }

            assert!(compare_coinbase_addresses(
                ContractId::from([1u8; 32]),
                ContractId::from([1u8; 32])
            ));
            assert!(!compare_coinbase_addresses(
                ContractId::from([9u8; 32]),
                ContractId::from([1u8; 32])
            ));
            assert!(!compare_coinbase_addresses(
                ContractId::from([1u8; 32]),
                ContractId::from([9u8; 32])
            ));
            assert!(compare_coinbase_addresses(
                ContractId::from([9u8; 32]),
                ContractId::from([9u8; 32])
            ));
        }

        #[test]
        fn invalidate_unexpected_index() {
            let mint = Transaction::mint(
                TxPointer::new(Default::default(), 1),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            );

            let mut block = Block::default();
            *block.transactions_mut() = vec![mint.into()];
            block.header_mut().recalculate_metadata();

            let validator = create_executor(
                Default::default(),
                Config {
                    utxo_validation_default: false,
                    ..Default::default()
                },
            );
            let validation_err = validator
                .execute_and_commit(ExecutionBlock::Validation(block), Default::default())
                .expect_err("Expected error because coinbase if invalid");
            assert!(matches!(
                validation_err,
                ExecutorError::MintHasUnexpectedIndex
            ));
        }

        #[test]
        fn invalidate_is_not_last() {
            let mint = Transaction::mint(
                TxPointer::new(Default::default(), 0),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            );
            let tx = Transaction::default_test_tx();

            let mut block = Block::default();
            *block.transactions_mut() = vec![mint.into(), tx];
            block.header_mut().recalculate_metadata();

            let validator = create_executor(Default::default(), Default::default());
            let validation_err = validator
                .execute_and_commit(ExecutionBlock::Validation(block), Default::default())
                .expect_err("Expected error because coinbase if invalid");
            assert!(matches!(
                validation_err,
                ExecutorError::MintIsNotLastTransaction
            ));
        }

        #[test]
        fn invalidate_block_missed_coinbase() {
            let block = Block::default();

            let validator = create_executor(Default::default(), Default::default());
            let validation_err = validator
                .execute_and_commit(ExecutionBlock::Validation(block), Default::default())
                .expect_err("Expected error because coinbase is missing");
            assert!(matches!(validation_err, ExecutorError::MintMissing));
        }

        #[test]
        fn invalidate_block_height() {
            let mint = Transaction::mint(
                TxPointer::new(1.into(), Default::default()),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            );

            let mut block = Block::default();
            *block.transactions_mut() = vec![mint.into()];
            block.header_mut().recalculate_metadata();

            let validator = create_executor(Default::default(), Default::default());
            let validation_err = validator
                .execute_and_commit(ExecutionBlock::Validation(block), Default::default())
                .expect_err("Expected error because coinbase if invalid");

            assert!(matches!(
                validation_err,
                ExecutorError::InvalidTransaction(CheckError::Validity(
                    ValidityError::TransactionMintIncorrectBlockHeight
                ))
            ));
        }

        #[test]
        fn invalidate_invalid_base_asset() {
            let mint = Transaction::mint(
                TxPointer::new(Default::default(), Default::default()),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            );

            let mut block = Block::default();
            *block.transactions_mut() = vec![mint.into()];
            block.header_mut().recalculate_metadata();

            let mut config = Config::default();
            config.consensus_parameters.base_asset_id = [1u8; 32].into();
            let validator = create_executor(Default::default(), config);
            let validation_err = validator
                .execute_and_commit(ExecutionBlock::Validation(block), Default::default())
                .expect_err("Expected error because coinbase if invalid");

            assert!(matches!(
                validation_err,
                ExecutorError::InvalidTransaction(CheckError::Validity(
                    ValidityError::TransactionMintNonBaseAsset
                ))
            ));
        }

        #[test]
        fn invalidate_mismatch_amount() {
            let mint = Transaction::mint(
                TxPointer::new(Default::default(), Default::default()),
                Default::default(),
                Default::default(),
                123,
                Default::default(),
            );

            let mut block = Block::default();
            *block.transactions_mut() = vec![mint.into()];
            block.header_mut().recalculate_metadata();

            let validator = create_executor(Default::default(), Default::default());
            let validation_err = validator
                .execute_and_commit(ExecutionBlock::Validation(block), Default::default())
                .expect_err("Expected error because coinbase if invalid");
            assert!(matches!(
                validation_err,
                ExecutorError::CoinbaseAmountMismatch
            ));
        }
    }

    // Ensure tx has at least one input to cover gas
    #[test]
    fn executor_invalidates_missing_gas_input() {
        let mut rng = StdRng::seed_from_u64(2322u64);
        let producer = create_executor(Default::default(), Default::default());
        let consensus_parameters = &producer.config.consensus_parameters;

        let verifier = create_executor(Default::default(), Default::default());

        let gas_limit = 100;
        let gas_price = 1;
        let script = TransactionBuilder::script(vec![], vec![])
            .add_unsigned_coin_input(
                SecretKey::random(&mut rng),
                rng.gen(),
                rng.gen(),
                rng.gen(),
                Default::default(),
                Default::default(),
            )
            .script_gas_limit(gas_limit)
            .gas_price(gas_price)
            .finalize();
        let max_fee: u64 = script
            .max_fee(
                consensus_parameters.gas_costs(),
                consensus_parameters.fee_params(),
            )
            .try_into()
            .unwrap();
        let tx: Transaction = script.into();

        let mut block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx.clone()],
        };

        let ExecutionData {
            skipped_transactions,
            ..
        } = producer
            .execute_block(
                ExecutionType::Production(PartialBlockComponent::from_partial_block(
                    &mut block,
                )),
                Default::default(),
            )
            .unwrap();
        let produce_result = &skipped_transactions[0].1;
        assert!(matches!(
            produce_result,
            &ExecutorError::InvalidTransaction(
                CheckError::Validity(
                    ValidityError::InsufficientFeeAmount { expected, .. }
                )
            ) if expected == max_fee
        ));

        // Produced block is valid
        verifier
            .execute_block(
                ExecutionType::Validation(PartialBlockComponent::from_partial_block(
                    &mut block,
                )),
                Default::default(),
            )
            .unwrap();

        // Invalidate the block with Insufficient tx
        block.transactions.insert(block.transactions.len() - 1, tx);
        let verify_result = verifier.execute_block(
            ExecutionType::Validation(PartialBlockComponent::from_partial_block(
                &mut block,
            )),
            Default::default(),
        );
        assert!(matches!(
            verify_result,
            Err(ExecutorError::InvalidTransaction(
                CheckError::Validity(
                    ValidityError::InsufficientFeeAmount { expected, .. }
                )
            )) if expected == max_fee
        ))
    }

    #[test]
    fn executor_invalidates_duplicate_tx_id() {
        let producer = create_executor(Default::default(), Default::default());

        let verifier = create_executor(Default::default(), Default::default());

        let mut block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![
                Transaction::default_test_tx(),
                Transaction::default_test_tx(),
            ],
        };

        let ExecutionData {
            skipped_transactions,
            ..
        } = producer
            .execute_block(
                ExecutionType::Production(PartialBlockComponent::from_partial_block(
                    &mut block,
                )),
                Default::default(),
            )
            .unwrap();
        let produce_result = &skipped_transactions[0].1;
        assert!(matches!(
            produce_result,
            &ExecutorError::TransactionIdCollision(_)
        ));

        // Produced block is valid
        verifier
            .execute_block(
                ExecutionType::Validation(PartialBlockComponent::from_partial_block(
                    &mut block,
                )),
                Default::default(),
            )
            .unwrap();

        // Make the block invalid by adding of the duplicating transaction
        block
            .transactions
            .insert(block.transactions.len() - 1, Transaction::default_test_tx());
        let verify_result = verifier.execute_block(
            ExecutionType::Validation(PartialBlockComponent::from_partial_block(
                &mut block,
            )),
            Default::default(),
        );
        assert!(matches!(
            verify_result,
            Err(ExecutorError::TransactionIdCollision(_))
        ));
    }

    // invalidate a block if a tx input doesn't exist
    #[test]
    fn executor_invalidates_missing_inputs() {
        // create an input which doesn't exist in the utxo set
        let mut rng = StdRng::seed_from_u64(2322u64);

        let tx = TransactionBuilder::script(
            vec![op::ret(RegId::ONE)].into_iter().collect(),
            vec![],
        )
        .add_unsigned_coin_input(
            SecretKey::random(&mut rng),
            rng.gen(),
            10,
            Default::default(),
            Default::default(),
            Default::default(),
        )
        .add_output(Output::Change {
            to: Default::default(),
            amount: 0,
            asset_id: Default::default(),
        })
        .finalize_as_transaction();

        // setup executors with utxo-validation enabled
        let config = Config {
            utxo_validation_default: true,
            ..Default::default()
        };
        let producer = create_executor(Database::default(), config.clone());

        let verifier = create_executor(Default::default(), config);

        let mut block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx.clone()],
        };

        let ExecutionData {
            skipped_transactions,
            ..
        } = producer
            .execute_block(
                ExecutionType::Production(PartialBlockComponent::from_partial_block(
                    &mut block,
                )),
                ExecutionOptions {
                    utxo_validation: true,
                },
            )
            .unwrap();
        let produce_result = &skipped_transactions[0].1;
        assert!(matches!(
            produce_result,
            &ExecutorError::TransactionValidity(
                TransactionValidityError::CoinDoesNotExist(_)
            )
        ));

        // Produced block is valid
        verifier
            .execute_block(
                ExecutionType::Validation(PartialBlockComponent::from_partial_block(
                    &mut block,
                )),
                ExecutionOptions {
                    utxo_validation: true,
                },
            )
            .unwrap();

        // Invalidate block by adding transaction with not existing coin
        block.transactions.insert(block.transactions.len() - 1, tx);
        let verify_result = verifier.execute_block(
            ExecutionType::Validation(PartialBlockComponent::from_partial_block(
                &mut block,
            )),
            ExecutionOptions {
                utxo_validation: true,
            },
        );
        assert!(matches!(
            verify_result,
            Err(ExecutorError::TransactionValidity(
                TransactionValidityError::CoinDoesNotExist(_)
            ))
        ));
    }

    // corrupt a produced block by randomizing change amount
    // and verify that the executor invalidates the tx
    #[test]
    fn executor_invalidates_blocks_with_diverging_tx_outputs() {
        let input_amount = 10;
        let fake_output_amount = 100;

        let tx: Transaction = TxBuilder::new(2322u64)
            .script_gas_limit(1)
            .coin_input(Default::default(), input_amount)
            .change_output(Default::default())
            .build()
            .transaction()
            .clone()
            .into();

        let tx_id = tx.id(&ChainId::default());

        let producer = create_executor(Default::default(), Default::default());

        let verifier = create_executor(Default::default(), Default::default());

        let mut block = Block::default();
        *block.transactions_mut() = vec![tx];

        let ExecutionResult { mut block, .. } = producer
            .execute_and_commit(
                ExecutionBlock::Production(block.into()),
                Default::default(),
            )
            .unwrap();

        // modify change amount
        if let Transaction::Script(script) = &mut block.transactions_mut()[0] {
            if let Output::Change { amount, .. } = &mut script.outputs_mut()[0] {
                *amount = fake_output_amount
            }
        }

        let verify_result = verifier
            .execute_and_commit(ExecutionBlock::Validation(block), Default::default());
        assert!(matches!(
            verify_result,
            Err(ExecutorError::InvalidTransactionOutcome { transaction_id }) if transaction_id == tx_id
        ));
    }

    // corrupt the merkle sum tree commitment from a produced block and verify that the
    // validation logic will reject the block
    #[test]
    fn executor_invalidates_blocks_with_diverging_tx_commitment() {
        let mut rng = StdRng::seed_from_u64(2322u64);
        let tx: Transaction = TxBuilder::new(2322u64)
            .script_gas_limit(1)
            .coin_input(Default::default(), 10)
            .change_output(Default::default())
            .build()
            .transaction()
            .clone()
            .into();

        let producer = create_executor(Default::default(), Default::default());

        let verifier = create_executor(Default::default(), Default::default());

        let mut block = Block::default();
        *block.transactions_mut() = vec![tx];

        let ExecutionResult { mut block, .. } = producer
            .execute_and_commit(
                ExecutionBlock::Production(block.into()),
                Default::default(),
            )
            .unwrap();

        // randomize transaction commitment
        block.header_mut().set_transaction_root(rng.gen());
        block.header_mut().recalculate_metadata();

        let verify_result = verifier
            .execute_and_commit(ExecutionBlock::Validation(block), Default::default());

        assert!(matches!(verify_result, Err(ExecutorError::InvalidBlockId)))
    }

    // invalidate a block if a tx is missing at least one coin input
    #[test]
    fn executor_invalidates_missing_coin_input() {
        let tx: Transaction = Transaction::default();

        let executor = create_executor(
            Database::default(),
            Config {
                utxo_validation_default: true,
                ..Default::default()
            },
        );

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        let ExecutionResult {
            skipped_transactions,
            ..
        } = executor
            .execute_and_commit(
                ExecutionBlock::Production(block),
                ExecutionOptions {
                    utxo_validation: true,
                },
            )
            .unwrap();

        let err = &skipped_transactions[0].1;
        // assert block failed to validate when transaction didn't contain any coin inputs
        assert!(matches!(
            err,
            &ExecutorError::InvalidTransaction(CheckError::Validity(
                ValidityError::NoSpendableInput
            ))
        ));
    }

    #[test]
    fn skipped_tx_not_changed_spent_status() {
        // `tx2` has two inputs: one used by `tx1` and on random. So after the execution of `tx1`,
        // the `tx2` become invalid and should be skipped by the block producers. Skipped
        // transactions should not affect the state so the second input should be `Unspent`.
        // # Dev-note: `TxBuilder::new(2322u64)` is used to create transactions, it produces
        // the same first input.
        let tx1 = TxBuilder::new(2322u64)
            .coin_input(AssetId::default(), 100)
            .change_output(AssetId::default())
            .build()
            .transaction()
            .clone();

        let tx2 = TxBuilder::new(2322u64)
            // The same input as `tx1`
            .coin_input(AssetId::default(), 100)
            // Additional unique for `tx2` input
            .coin_input(AssetId::default(), 100)
            .change_output(AssetId::default())
            .build()
            .transaction()
            .clone();

        let first_input = tx2.inputs()[0].clone();
        let mut first_coin = CompressedCoin::default();
        first_coin.set_owner(*first_input.input_owner().unwrap());
        first_coin.set_amount(100);
        let second_input = tx2.inputs()[1].clone();
        let mut second_coin = CompressedCoin::default();
        second_coin.set_owner(*second_input.input_owner().unwrap());
        second_coin.set_amount(100);
        let db = &mut Database::default();
        // Insert both inputs
        db.storage::<Coins>()
            .insert(&first_input.utxo_id().unwrap().clone(), &first_coin)
            .unwrap();
        db.storage::<Coins>()
            .insert(&second_input.utxo_id().unwrap().clone(), &second_coin)
            .unwrap();
        let executor = create_executor(
            db.clone(),
            Config {
                utxo_validation_default: true,
                ..Default::default()
            },
        );

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx1.into(), tx2.clone().into()],
        };

        // The first input should be `Unspent` before execution.
        db.storage::<Coins>()
            .get(first_input.utxo_id().unwrap())
            .unwrap()
            .expect("coin should be unspent");
        // The second input should be `Unspent` before execution.
        db.storage::<Coins>()
            .get(second_input.utxo_id().unwrap())
            .unwrap()
            .expect("coin should be unspent");

        let ExecutionResult {
            block,
            skipped_transactions,
            ..
        } = executor
            .execute_and_commit(
                ExecutionBlock::Production(block),
                ExecutionOptions {
                    utxo_validation: true,
                },
            )
            .unwrap();
        // `tx2` should be skipped.
        assert_eq!(block.transactions().len(), 2 /* coinbase and `tx1` */);
        assert_eq!(skipped_transactions.len(), 1);
        assert_eq!(skipped_transactions[0].0, tx2.id(&ChainId::default()));

        // The first input should be spent by `tx1` after execution.
        let coin = db
            .storage::<Coins>()
            .get(first_input.utxo_id().unwrap())
            .unwrap();
        // verify coin is pruned from utxo set
        assert!(coin.is_none());
        // The second input should be `Unspent` after execution.
        db.storage::<Coins>()
            .get(second_input.utxo_id().unwrap())
            .unwrap()
            .expect("coin should be unspent");
    }

    #[test]
    fn coin_input_fails_when_mismatches_database() {
        const AMOUNT: u64 = 100;

        let tx = TxBuilder::new(2322u64)
            .coin_input(AssetId::default(), AMOUNT)
            .change_output(AssetId::default())
            .build()
            .transaction()
            .clone();

        let input = tx.inputs()[0].clone();
        let mut coin = CompressedCoin::default();
        coin.set_owner(*input.input_owner().unwrap());
        coin.set_amount(AMOUNT - 1);
        let db = &mut Database::default();

        // Inserting a coin with `AMOUNT - 1` should cause a mismatching error during production.
        db.storage::<Coins>()
            .insert(&input.utxo_id().unwrap().clone(), &coin)
            .unwrap();
        let executor = create_executor(
            db.clone(),
            Config {
                utxo_validation_default: true,
                ..Default::default()
            },
        );

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx.into()],
        };

        let ExecutionResult {
            skipped_transactions,
            ..
        } = executor
            .execute_and_commit(
                ExecutionBlock::Production(block),
                ExecutionOptions {
                    utxo_validation: true,
                },
            )
            .unwrap();
        // `tx` should be skipped.
        assert_eq!(skipped_transactions.len(), 1);
        let err = &skipped_transactions[0].1;
        assert!(matches!(
            err,
            &ExecutorError::TransactionValidity(TransactionValidityError::CoinMismatch(
                _
            ))
        ));
    }

    #[test]
    fn contract_input_fails_when_doesnt_exist_in_database() {
        let contract_id: ContractId = [1; 32].into();
        let tx = TxBuilder::new(2322u64)
            .contract_input(contract_id)
            .coin_input(AssetId::default(), 100)
            .change_output(AssetId::default())
            .contract_output(&contract_id)
            .build()
            .transaction()
            .clone();

        let input = tx.inputs()[1].clone();
        let mut coin = CompressedCoin::default();
        coin.set_owner(*input.input_owner().unwrap());
        coin.set_amount(100);
        let db = &mut Database::default();

        db.storage::<Coins>()
            .insert(&input.utxo_id().unwrap().clone(), &coin)
            .unwrap();
        let executor = create_executor(
            db.clone(),
            Config {
                utxo_validation_default: true,
                ..Default::default()
            },
        );

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx.into()],
        };

        let ExecutionResult {
            skipped_transactions,
            ..
        } = executor
            .execute_and_commit(
                ExecutionBlock::Production(block),
                ExecutionOptions {
                    utxo_validation: true,
                },
            )
            .unwrap();
        // `tx` should be skipped.
        assert_eq!(skipped_transactions.len(), 1);
        let err = &skipped_transactions[0].1;
        assert!(matches!(
            err,
            &ExecutorError::TransactionValidity(
                TransactionValidityError::ContractDoesNotExist(_)
            )
        ));
    }

    #[test]
    fn skipped_txs_not_affect_order() {
        // `tx1` is invalid because it doesn't have inputs for gas.
        // `tx2` is a `Create` transaction with some code inside.
        // `tx3` is a `Script` transaction that depends on `tx2`. It will be skipped
        // if `tx2` is not executed before `tx3`.
        //
        // The test checks that execution for the block with transactions [tx1, tx2, tx3] skips
        // transaction `tx1` and produce a block [tx2, tx3] with the expected order.
        let tx1 = TransactionBuilder::script(vec![], vec![])
            .add_random_fee_input()
            .script_gas_limit(1000000)
            .gas_price(1000000)
            .finalize_as_transaction();
        let (tx2, tx3) = setup_executable_script();

        let executor = create_executor(Default::default(), Default::default());

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx1.clone(), tx2.clone().into(), tx3.clone().into()],
        };

        let ExecutionResult {
            block,
            skipped_transactions,
            ..
        } = executor
            .execute_and_commit(ExecutionBlock::Production(block), Default::default())
            .unwrap();
        assert_eq!(
            block.transactions().len(),
            3 // coinbase, `tx2` and `tx3`
        );
        assert_eq!(
            block.transactions()[0].id(&ChainId::default()),
            tx2.id(&ChainId::default())
        );
        assert_eq!(
            block.transactions()[1].id(&ChainId::default()),
            tx3.id(&ChainId::default())
        );
        // `tx1` should be skipped.
        assert_eq!(skipped_transactions.len(), 1);
        assert_eq!(&skipped_transactions[0].0, &tx1.id(&ChainId::default()));
        let tx2_index_in_the_block =
            block.transactions()[1].as_script().unwrap().inputs()[0]
                .tx_pointer()
                .unwrap()
                .tx_index();
        assert_eq!(tx2_index_in_the_block, 0);
    }

    #[test]
    fn input_coins_are_marked_as_spent() {
        // ensure coins are marked as spent after tx is processed
        let tx: Transaction = TxBuilder::new(2322u64)
            .coin_input(AssetId::default(), 100)
            .change_output(AssetId::default())
            .build()
            .transaction()
            .clone()
            .into();

        let mut db = &Database::default();
        let executor = create_executor(db.clone(), Default::default());

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        let ExecutionResult { block, .. } = executor
            .execute_and_commit(ExecutionBlock::Production(block), Default::default())
            .unwrap();

        // assert the tx coin is spent
        let coin = db
            .storage::<Coins>()
            .get(
                block.transactions()[0].as_script().unwrap().inputs()[0]
                    .utxo_id()
                    .unwrap(),
            )
            .unwrap();
        // spent coins should be removed
        assert!(coin.is_none());
    }

    #[test]
    fn contracts_balance_and_state_roots_no_modifications_updated() {
        // Values in inputs and outputs are random. If the execution of the transaction successful,
        // it should actualize them to use a valid the balance and state roots. Because it is not
        // changes, the balance the root should be default - `[0; 32]`.
        let mut rng = StdRng::seed_from_u64(2322u64);

        let (create, contract_id) = create_contract(vec![], &mut rng);
        let non_modify_state_tx: Transaction = TxBuilder::new(2322)
            .script_gas_limit(10000)
            .coin_input(AssetId::zeroed(), 10000)
            .start_script(vec![op::ret(1)], vec![])
            .contract_input(contract_id)
            .fee_input()
            .contract_output(&contract_id)
            .build()
            .transaction()
            .clone()
            .into();
        let db = &mut Database::default();

        let executor = create_executor(
            db.clone(),
            Config {
                utxo_validation_default: false,
                ..Default::default()
            },
        );

        let block = PartialFuelBlock {
            header: PartialBlockHeader {
                consensus: ConsensusHeader {
                    height: 1.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions: vec![create.into(), non_modify_state_tx],
        };

        let ExecutionResult {
            block, tx_status, ..
        } = executor
            .execute_and_commit(ExecutionBlock::Production(block), Default::default())
            .unwrap();

        // Assert the balance and state roots should be the same before and after execution.
        let empty_state = (*sparse::empty_sum()).into();
        let executed_tx = block.transactions()[1].as_script().unwrap();
        assert!(matches!(
            tx_status[2].result,
            TransactionExecutionResult::Success { .. }
        ));
        assert_eq!(executed_tx.inputs()[0].state_root(), Some(&empty_state));
        assert_eq!(executed_tx.inputs()[0].balance_root(), Some(&empty_state));
        assert_eq!(executed_tx.outputs()[0].state_root(), Some(&empty_state));
        assert_eq!(executed_tx.outputs()[0].balance_root(), Some(&empty_state));
    }

    #[test]
    fn contracts_balance_and_state_roots_updated_no_modifications_on_fail() {
        // Values in inputs and outputs are random. If the execution of the transaction fails,
        // it still should actualize them to use the balance and state roots before the execution.
        let mut rng = StdRng::seed_from_u64(2322u64);

        let (create, contract_id) = create_contract(vec![], &mut rng);
        // The transaction with invalid script.
        let non_modify_state_tx: Transaction = TxBuilder::new(2322)
            .start_script(vec![op::add(RegId::PC, RegId::PC, RegId::PC)], vec![])
            .contract_input(contract_id)
            .fee_input()
            .contract_output(&contract_id)
            .build()
            .transaction()
            .clone()
            .into();
        let db = &mut Database::default();

        let executor = create_executor(
            db.clone(),
            Config {
                utxo_validation_default: false,
                ..Default::default()
            },
        );

        let block = PartialFuelBlock {
            header: PartialBlockHeader {
                consensus: ConsensusHeader {
                    height: 1.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions: vec![create.into(), non_modify_state_tx],
        };

        let ExecutionResult {
            block, tx_status, ..
        } = executor
            .execute_and_commit(ExecutionBlock::Production(block), Default::default())
            .unwrap();

        // Assert the balance and state roots should be the same before and after execution.
        let empty_state = (*sparse::empty_sum()).into();
        let executed_tx = block.transactions()[1].as_script().unwrap();
        assert!(matches!(
            tx_status[1].result,
            TransactionExecutionResult::Failed { .. }
        ));
        assert_eq!(
            executed_tx.inputs()[0].state_root(),
            executed_tx.outputs()[0].state_root()
        );
        assert_eq!(
            executed_tx.inputs()[0].balance_root(),
            executed_tx.outputs()[0].balance_root()
        );
        assert_eq!(executed_tx.inputs()[0].state_root(), Some(&empty_state));
        assert_eq!(executed_tx.inputs()[0].balance_root(), Some(&empty_state));
    }

    #[test]
    fn contracts_balance_and_state_roots_updated_modifications_updated() {
        // Values in inputs and outputs are random. If the execution of the transaction that
        // modifies the state and the balance is successful, it should update roots.
        let mut rng = StdRng::seed_from_u64(2322u64);

        // Create a contract that modifies the state
        let (create, contract_id) = create_contract(
            vec![
                // Sets the state STATE[0x1; 32] = value of `RegId::PC`;
                op::sww(0x1, 0x29, RegId::PC),
                op::ret(1),
            ]
            .into_iter()
            .collect::<Vec<u8>>(),
            &mut rng,
        );

        let transfer_amount = 100 as Word;
        let asset_id = AssetId::from([2; 32]);
        let (script, data_offset) = script_with_data_offset!(
            data_offset,
            vec![
                // Set register `0x10` to `Call`
                op::movi(0x10, data_offset + AssetId::LEN as u32),
                // Set register `0x11` with offset to data that contains `asset_id`
                op::movi(0x11, data_offset),
                // Set register `0x12` with `transfer_amount`
                op::movi(0x12, transfer_amount as u32),
                op::call(0x10, 0x12, 0x11, RegId::CGAS),
                op::ret(RegId::ONE),
            ],
            TxParameters::DEFAULT.tx_offset()
        );

        let script_data: Vec<u8> = [
            asset_id.as_ref(),
            Call::new(contract_id, transfer_amount, data_offset as Word)
                .to_bytes()
                .as_ref(),
        ]
        .into_iter()
        .flatten()
        .copied()
        .collect();

        let modify_balance_and_state_tx = TxBuilder::new(2322)
            .script_gas_limit(10000)
            .coin_input(AssetId::zeroed(), 10000)
            .start_script(script, script_data)
            .contract_input(contract_id)
            .coin_input(asset_id, transfer_amount)
            .fee_input()
            .contract_output(&contract_id)
            .build()
            .transaction()
            .clone();
        let db = &mut Database::default();

        let executor = create_executor(
            db.clone(),
            Config {
                utxo_validation_default: false,
                ..Default::default()
            },
        );

        let block = PartialFuelBlock {
            header: PartialBlockHeader {
                consensus: ConsensusHeader {
                    height: 1.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions: vec![create.into(), modify_balance_and_state_tx.into()],
        };

        let ExecutionResult {
            block, tx_status, ..
        } = executor
            .execute_and_commit(ExecutionBlock::Production(block), Default::default())
            .unwrap();

        let empty_state = (*sparse::empty_sum()).into();
        let executed_tx = block.transactions()[1].as_script().unwrap();
        assert!(matches!(
            tx_status[2].result,
            TransactionExecutionResult::Success { .. }
        ));
        assert_eq!(executed_tx.inputs()[0].state_root(), Some(&empty_state));
        assert_eq!(executed_tx.inputs()[0].balance_root(), Some(&empty_state));
        // Roots should be different
        assert_ne!(
            executed_tx.inputs()[0].state_root(),
            executed_tx.outputs()[0].state_root()
        );
        assert_ne!(
            executed_tx.inputs()[0].balance_root(),
            executed_tx.outputs()[0].balance_root()
        );
    }

    #[test]
    fn contracts_balance_and_state_roots_in_inputs_updated() {
        // Values in inputs and outputs are random. If the execution of the transaction that
        // modifies the state and the balance is successful, it should update roots.
        // The first transaction updates the `balance_root` and `state_root`.
        // The second transaction is empty. The executor should update inputs of the second
        // transaction with the same value from `balance_root` and `state_root`.
        let mut rng = StdRng::seed_from_u64(2322u64);

        // Create a contract that modifies the state
        let (create, contract_id) = create_contract(
            vec![
                // Sets the state STATE[0x1; 32] = value of `RegId::PC`;
                op::sww(0x1, 0x29, RegId::PC),
                op::ret(1),
            ]
            .into_iter()
            .collect::<Vec<u8>>(),
            &mut rng,
        );

        let transfer_amount = 100 as Word;
        let asset_id = AssetId::from([2; 32]);
        let (script, data_offset) = script_with_data_offset!(
            data_offset,
            vec![
                // Set register `0x10` to `Call`
                op::movi(0x10, data_offset + AssetId::LEN as u32),
                // Set register `0x11` with offset to data that contains `asset_id`
                op::movi(0x11, data_offset),
                // Set register `0x12` with `transfer_amount`
                op::movi(0x12, transfer_amount as u32),
                op::call(0x10, 0x12, 0x11, RegId::CGAS),
                op::ret(RegId::ONE),
            ],
            TxParameters::DEFAULT.tx_offset()
        );

        let script_data: Vec<u8> = [
            asset_id.as_ref(),
            Call::new(contract_id, transfer_amount, data_offset as Word)
                .to_bytes()
                .as_ref(),
        ]
        .into_iter()
        .flatten()
        .copied()
        .collect();

        let modify_balance_and_state_tx = TxBuilder::new(2322)
            .script_gas_limit(10000)
            .coin_input(AssetId::zeroed(), 10000)
            .start_script(script, script_data)
            .contract_input(contract_id)
            .coin_input(asset_id, transfer_amount)
            .fee_input()
            .contract_output(&contract_id)
            .build()
            .transaction()
            .clone();
        let db = &mut Database::default();

        let executor = create_executor(
            db.clone(),
            Config {
                utxo_validation_default: false,
                ..Default::default()
            },
        );

        let block = PartialFuelBlock {
            header: PartialBlockHeader {
                consensus: ConsensusHeader {
                    height: 1.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions: vec![create.into(), modify_balance_and_state_tx.into()],
        };

        let ExecutionResult { block, .. } = executor
            .execute_and_commit(ExecutionBlock::Production(block), Default::default())
            .unwrap();

        let executed_tx = block.transactions()[1].as_script().unwrap();
        let state_root = executed_tx.outputs()[0].state_root();
        let balance_root = executed_tx.outputs()[0].balance_root();

        let mut new_tx = executed_tx.clone();
        *new_tx.script_mut() = vec![];
        new_tx
            .precompute(&executor.config.consensus_parameters.chain_id)
            .unwrap();

        let block = PartialFuelBlock {
            header: PartialBlockHeader {
                consensus: ConsensusHeader {
                    height: 2.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions: vec![new_tx.into()],
        };

        let ExecutionResult {
            block, tx_status, ..
        } = executor
            .execute_and_commit(ExecutionBlock::Production(block), Default::default())
            .unwrap();
        assert!(matches!(
            tx_status[1].result,
            TransactionExecutionResult::Success { .. }
        ));
        let tx = block.transactions()[0].as_script().unwrap();
        assert_eq!(tx.inputs()[0].balance_root(), balance_root);
        assert_eq!(tx.inputs()[0].state_root(), state_root);
    }

    #[test]
    fn foreign_transfer_should_not_affect_balance_root() {
        // The foreign transfer of tokens should not affect the balance root of the transaction.
        let mut rng = StdRng::seed_from_u64(2322u64);

        let (create, contract_id) = create_contract(vec![], &mut rng);

        let transfer_amount = 100 as Word;
        let asset_id = AssetId::from([2; 32]);
        let mut foreign_transfer = TxBuilder::new(2322)
            .script_gas_limit(10000)
            .coin_input(AssetId::zeroed(), 10000)
            .start_script(vec![op::ret(1)], vec![])
            .coin_input(asset_id, transfer_amount)
            .coin_output(asset_id, transfer_amount)
            .build()
            .transaction()
            .clone();
        if let Some(Output::Coin { to, .. }) = foreign_transfer
            .as_script_mut()
            .unwrap()
            .outputs_mut()
            .last_mut()
        {
            *to = Address::try_from(contract_id.as_ref()).unwrap();
        } else {
            panic!("Last outputs should be a coin for the contract");
        }
        let db = &mut Database::default();

        let executor = create_executor(db.clone(), Default::default());

        let block = PartialFuelBlock {
            header: PartialBlockHeader {
                consensus: ConsensusHeader {
                    height: 1.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions: vec![create.into(), foreign_transfer.into()],
        };

        let _ = executor
            .execute_and_commit(ExecutionBlock::Production(block), Default::default())
            .unwrap();

        // Assert the balance root should not be affected.
        let empty_state = (*sparse::empty_sum()).into();
        assert_eq!(
            ContractRef::new(db, contract_id).balance_root().unwrap(),
            empty_state
        );
    }

    #[test]
    fn input_coins_are_marked_as_spent_with_utxo_validation_enabled() {
        // ensure coins are marked as spent after tx is processed
        let mut rng = StdRng::seed_from_u64(2322u64);
        let starting_block = BlockHeight::from(5);
        let starting_block_tx_idx = Default::default();

        let tx = TransactionBuilder::script(
            vec![op::ret(RegId::ONE)].into_iter().collect(),
            vec![],
        )
        .add_unsigned_coin_input(
            SecretKey::random(&mut rng),
            rng.gen(),
            100,
            Default::default(),
            Default::default(),
            Default::default(),
        )
        .add_output(Output::Change {
            to: Default::default(),
            amount: 0,
            asset_id: Default::default(),
        })
        .finalize();
        let db = &mut Database::default();

        // insert coin into state
        if let Input::CoinSigned(CoinSigned {
            utxo_id,
            owner,
            amount,
            asset_id,
            ..
        }) = tx.inputs()[0]
        {
            let mut coin = CompressedCoin::default();
            coin.set_owner(owner);
            coin.set_amount(amount);
            coin.set_asset_id(asset_id);
            coin.set_tx_pointer(TxPointer::new(starting_block, starting_block_tx_idx));
            db.storage::<Coins>().insert(&utxo_id, &coin).unwrap();
        }

        let executor = create_executor(
            db.clone(),
            Config {
                utxo_validation_default: true,
                ..Default::default()
            },
        );

        let block = PartialFuelBlock {
            header: PartialBlockHeader {
                consensus: ConsensusHeader {
                    height: 6.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions: vec![tx.into()],
        };

        let ExecutionResult { block, events, .. } = executor
            .execute_and_commit(
                ExecutionBlock::Production(block),
                ExecutionOptions {
                    utxo_validation: true,
                },
            )
            .unwrap();

        // assert the tx coin is spent
        let utxo_id = block.transactions()[0].as_script().unwrap().inputs()[0]
            .utxo_id()
            .unwrap();
        let coin = db.storage::<Coins>().get(utxo_id).unwrap();
        assert!(coin.is_none());
        assert_eq!(events.len(), 2);
        assert!(
            matches!(events[0], ExecutorEvent::CoinConsumed(spent_coin) if &spent_coin.utxo_id == utxo_id)
        );
        assert!(matches!(events[1], ExecutorEvent::CoinCreated(_)));
    }

    #[test]
    fn validation_succeeds_when_input_contract_utxo_id_uses_expected_value() {
        let mut rng = StdRng::seed_from_u64(2322);
        // create a contract in block 1
        // verify a block 2 with tx containing contract id from block 1, using the correct contract utxo_id from block 1.
        let (tx, contract_id) = create_contract(vec![], &mut rng);
        let first_block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx.into()],
        };

        let tx2: Transaction = TxBuilder::new(2322)
            .start_script(vec![op::ret(1)], vec![])
            .contract_input(contract_id)
            .fee_input()
            .contract_output(&contract_id)
            .build()
            .transaction()
            .clone()
            .into();

        let second_block = PartialFuelBlock {
            header: PartialBlockHeader {
                consensus: ConsensusHeader {
                    height: 2.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions: vec![tx2],
        };

        let db = Database::default();

        let setup = create_executor(db.clone(), Default::default());

        setup
            .execute_and_commit(
                ExecutionBlock::Production(first_block),
                Default::default(),
            )
            .unwrap();

        let producer_view = db.transaction().deref_mut().clone();
        let producer = create_executor(producer_view, Default::default());
        let ExecutionResult {
            block: second_block,
            ..
        } = producer
            .execute_and_commit(
                ExecutionBlock::Production(second_block),
                Default::default(),
            )
            .unwrap();

        let verifier = create_executor(db, Default::default());
        let verify_result = verifier.execute_and_commit(
            ExecutionBlock::Validation(second_block),
            Default::default(),
        );
        assert!(verify_result.is_ok());
    }

    // verify that a contract input must exist for a transaction
    #[test]
    fn invalidates_if_input_contract_utxo_id_is_divergent() {
        let mut rng = StdRng::seed_from_u64(2322);

        // create a contract in block 1
        // verify a block 2 containing contract id from block 1, with wrong input contract utxo_id
        let (tx, contract_id) = create_contract(vec![], &mut rng);
        let tx2: Transaction = TxBuilder::new(2322)
            .start_script(vec![op::addi(0x10, RegId::ZERO, 0), op::ret(1)], vec![])
            .contract_input(contract_id)
            .fee_input()
            .contract_output(&contract_id)
            .build()
            .transaction()
            .clone()
            .into();

        let first_block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx.into(), tx2],
        };

        let tx3: Transaction = TxBuilder::new(2322)
            .start_script(vec![op::addi(0x10, RegId::ZERO, 1), op::ret(1)], vec![])
            .contract_input(contract_id)
            .fee_input()
            .contract_output(&contract_id)
            .build()
            .transaction()
            .clone()
            .into();
        let tx_id = tx3.id(&ChainId::default());

        let second_block = PartialFuelBlock {
            header: PartialBlockHeader {
                consensus: ConsensusHeader {
                    height: 2.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions: vec![tx3],
        };

        let db = Database::default();

        let setup = create_executor(db.clone(), Default::default());

        setup
            .execute_and_commit(
                ExecutionBlock::Production(first_block),
                Default::default(),
            )
            .unwrap();

        let producer_view = db.transaction().deref_mut().clone();
        let producer = create_executor(producer_view, Default::default());

        let ExecutionResult {
            block: mut second_block,
            ..
        } = producer
            .execute_and_commit(
                ExecutionBlock::Production(second_block),
                Default::default(),
            )
            .unwrap();
        // Corrupt the utxo_id of the contract output
        if let Transaction::Script(script) = &mut second_block.transactions_mut()[0] {
            if let Input::Contract(contract::Contract { utxo_id, .. }) =
                &mut script.inputs_mut()[0]
            {
                // use a previously valid contract id which isn't the correct one for this block
                *utxo_id = UtxoId::new(tx_id, 0);
            }
        }

        let verifier = create_executor(db, Default::default());
        let verify_result = verifier.execute_and_commit(
            ExecutionBlock::Validation(second_block),
            Default::default(),
        );

        assert!(matches!(
            verify_result,
            Err(ExecutorError::InvalidTransactionOutcome {
                transaction_id
            }) if transaction_id == tx_id
        ));
    }

    #[test]
    fn outputs_with_amount_are_included_utxo_set() {
        let (deploy, script) = setup_executable_script();
        let script_id = script.id(&ChainId::default());

        let mut database = &Database::default();
        let executor = create_executor(database.clone(), Default::default());

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![deploy.into(), script.into()],
        };

        let ExecutionResult { block, .. } = executor
            .execute_and_commit(ExecutionBlock::Production(block), Default::default())
            .unwrap();

        // ensure that all utxos with an amount are stored into the utxo set
        for (idx, output) in block.transactions()[1]
            .as_script()
            .unwrap()
            .outputs()
            .iter()
            .enumerate()
        {
            let id = UtxoId::new(script_id, idx as u8);
            match output {
                Output::Change { .. } | Output::Variable { .. } | Output::Coin { .. } => {
                    let maybe_utxo = database.storage::<Coins>().get(&id).unwrap();
                    assert!(maybe_utxo.is_some());
                    let utxo = maybe_utxo.unwrap();
                    assert!(*utxo.amount() > 0)
                }
                _ => (),
            }
        }
    }

    #[test]
    fn outputs_with_no_value_are_excluded_from_utxo_set() {
        let mut rng = StdRng::seed_from_u64(2322);
        let asset_id: AssetId = rng.gen();
        let input_amount = 0;
        let coin_output_amount = 0;

        let tx: Transaction = TxBuilder::new(2322)
            .coin_input(asset_id, input_amount)
            .variable_output(Default::default())
            .coin_output(asset_id, coin_output_amount)
            .change_output(asset_id)
            .build()
            .transaction()
            .clone()
            .into();
        let tx_id = tx.id(&ChainId::default());

        let mut database = &Database::default();
        let executor = create_executor(database.clone(), Default::default());

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        executor
            .execute_and_commit(ExecutionBlock::Production(block), Default::default())
            .unwrap();

        for idx in 0..2 {
            let id = UtxoId::new(tx_id, idx);
            let maybe_utxo = database.storage::<Coins>().get(&id).unwrap();
            assert!(maybe_utxo.is_none());
        }
    }

    fn message_from_input(input: &Input, da_height: u64) -> Message {
        MessageV1 {
            sender: *input.sender().unwrap(),
            recipient: *input.recipient().unwrap(),
            nonce: *input.nonce().unwrap(),
            amount: input.amount().unwrap(),
            data: input
                .input_data()
                .map(|data| data.to_vec())
                .unwrap_or_default(),
            da_height: DaBlockHeight(da_height),
        }
        .into()
    }

    /// Helper to build transactions and a message in it for some of the message tests
    fn make_tx_and_message(rng: &mut StdRng, da_height: u64) -> (Transaction, Message) {
        let tx = TransactionBuilder::script(vec![], vec![])
            .add_unsigned_message_input(
                SecretKey::random(rng),
                rng.gen(),
                rng.gen(),
                1000,
                vec![],
            )
            .finalize();

        let message = message_from_input(&tx.inputs()[0], da_height);
        (tx.into(), message)
    }

    /// Helper to build database and executor for some of the message tests
    fn make_executor(messages: &[&Message]) -> Executor<Database, DisabledRelayer> {
        let mut database = Database::default();
        let database_ref = &mut database;

        for message in messages {
            database_ref
                .storage::<Messages>()
                .insert(message.id(), message)
                .unwrap();
        }

        create_executor(
            database,
            Config {
                utxo_validation_default: true,
                ..Default::default()
            },
        )
    }

    #[test]
    fn unspent_message_succeeds_when_msg_da_height_lt_block_da_height() {
        let mut rng = StdRng::seed_from_u64(2322);

        let (tx, message) = make_tx_and_message(&mut rng, 0);

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        let ExecutionResult { block, .. } = make_executor(&[&message])
            .execute_and_commit(
                ExecutionBlock::Production(block),
                ExecutionOptions {
                    utxo_validation: true,
                },
            )
            .expect("block execution failed unexpectedly");

        make_executor(&[&message])
            .execute_and_commit(
                ExecutionBlock::Validation(block),
                ExecutionOptions {
                    utxo_validation: true,
                },
            )
            .expect("block validation failed unexpectedly");
    }

    #[test]
    fn successful_execution_consume_all_messages() {
        let mut rng = StdRng::seed_from_u64(2322);
        let to: Address = rng.gen();
        let amount = 500;

        let tx = TransactionBuilder::script(vec![], vec![])
            // Add `Input::MessageCoin`
            .add_unsigned_message_input(SecretKey::random(&mut rng), rng.gen(), rng.gen(), amount, vec![])
            // Add `Input::MessageData`
            .add_unsigned_message_input(SecretKey::random(&mut rng), rng.gen(), rng.gen(), amount, vec![0xff; 10])
            .add_output(Output::change(to, amount + amount, AssetId::BASE))
            .finalize();
        let tx_id = tx.id(&ChainId::default());

        let message_coin = message_from_input(&tx.inputs()[0], 0);
        let message_data = message_from_input(&tx.inputs()[1], 0);
        let messages = vec![&message_coin, &message_data];

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx.into()],
        };

        let exec = make_executor(&messages);
        let view = exec.database_view_provider.latest_view();
        assert!(!view.message_is_spent(message_coin.nonce()).unwrap());
        assert!(!view.message_is_spent(message_data.nonce()).unwrap());

        let ExecutionResult {
            skipped_transactions,
            ..
        } = exec
            .execute_and_commit(
                ExecutionBlock::Production(block),
                ExecutionOptions {
                    utxo_validation: true,
                },
            )
            .unwrap();
        assert_eq!(skipped_transactions.len(), 0);

        // Successful execution consumes `message_coin` and `message_data`.
        let view = exec.database_view_provider.latest_view();
        assert!(view.message_is_spent(message_coin.nonce()).unwrap());
        assert!(view.message_is_spent(message_data.nonce()).unwrap());
        assert_eq!(
            *view.coin(&UtxoId::new(tx_id, 0)).unwrap().amount(),
            amount + amount
        );
    }

    #[test]
    fn reverted_execution_consume_only_message_coins() {
        let mut rng = StdRng::seed_from_u64(2322);
        let to: Address = rng.gen();
        let amount = 500;

        // Script that return `1` - failed script -> execution result will be reverted.
        let script = vec![op::ret(1)].into_iter().collect();
        let tx = TransactionBuilder::script(script, vec![])
            // Add `Input::MessageCoin`
            .add_unsigned_message_input(SecretKey::random(&mut rng), rng.gen(), rng.gen(), amount, vec![])
            // Add `Input::MessageData`
            .add_unsigned_message_input(SecretKey::random(&mut rng), rng.gen(), rng.gen(), amount, vec![0xff; 10])
            .add_output(Output::change(to, amount + amount, AssetId::BASE))
            .finalize();
        let tx_id = tx.id(&ChainId::default());

        let message_coin = message_from_input(&tx.inputs()[0], 0);
        let message_data = message_from_input(&tx.inputs()[1], 0);
        let messages = vec![&message_coin, &message_data];

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx.into()],
        };

        let exec = make_executor(&messages);
        let view = exec.database_view_provider.latest_view();
        assert!(!view.message_is_spent(message_coin.nonce()).unwrap());
        assert!(!view.message_is_spent(message_data.nonce()).unwrap());

        let ExecutionResult {
            skipped_transactions,
            ..
        } = exec
            .execute_and_commit(
                ExecutionBlock::Production(block),
                ExecutionOptions {
                    utxo_validation: true,
                },
            )
            .unwrap();
        assert_eq!(skipped_transactions.len(), 0);

        // We should spend only `message_coin`. The `message_data` should be unspent.
        let view = exec.database_view_provider.latest_view();
        assert!(view.message_is_spent(message_coin.nonce()).unwrap());
        assert!(!view.message_is_spent(message_data.nonce()).unwrap());
        assert_eq!(*view.coin(&UtxoId::new(tx_id, 0)).unwrap().amount(), amount);
    }

    #[test]
    fn message_fails_when_spending_nonexistent_message_id() {
        let mut rng = StdRng::seed_from_u64(2322);

        let (tx, _message) = make_tx_and_message(&mut rng, 0);

        let mut block = Block::default();
        *block.transactions_mut() = vec![tx.clone()];

        let ExecutionResult {
            skipped_transactions,
            mut block,
            ..
        } = make_executor(&[]) // No messages in the db
            .execute_and_commit(
                ExecutionBlock::Production(block.clone().into()),
                ExecutionOptions {
                    utxo_validation: true,
                },
            )
            .unwrap();
        let err = &skipped_transactions[0].1;
        assert!(matches!(
            err,
            &ExecutorError::TransactionValidity(
                TransactionValidityError::MessageDoesNotExist(_)
            )
        ));

        // Produced block is valid
        make_executor(&[]) // No messages in the db
            .execute_and_commit(
                ExecutionBlock::Validation(block.clone()),
                ExecutionOptions {
                    utxo_validation: true,
                },
            )
            .unwrap();

        // Invalidate block by returning back `tx` with not existing message
        let index = block.transactions().len() - 1;
        block.transactions_mut().insert(index, tx);
        let res = make_executor(&[]) // No messages in the db
            .execute_and_commit(
                ExecutionBlock::Validation(block),
                ExecutionOptions {
                    utxo_validation: true,
                },
            );
        assert!(matches!(
            res,
            Err(ExecutorError::TransactionValidity(
                TransactionValidityError::MessageDoesNotExist(_)
            ))
        ));
    }

    #[test]
    fn message_fails_when_spending_da_height_gt_block_da_height() {
        let mut rng = StdRng::seed_from_u64(2322);

        let (tx, message) = make_tx_and_message(&mut rng, 1); // Block has zero da_height

        let mut block = Block::default();
        *block.transactions_mut() = vec![tx.clone()];

        let ExecutionResult {
            skipped_transactions,
            mut block,
            ..
        } = make_executor(&[&message])
            .execute_and_commit(
                ExecutionBlock::Production(block.clone().into()),
                ExecutionOptions {
                    utxo_validation: true,
                },
            )
            .unwrap();
        let err = &skipped_transactions[0].1;
        assert!(matches!(
            err,
            &ExecutorError::TransactionValidity(
                TransactionValidityError::MessageSpendTooEarly(_)
            )
        ));

        // Produced block is valid
        make_executor(&[&message])
            .execute_and_commit(
                ExecutionBlock::Validation(block.clone()),
                ExecutionOptions {
                    utxo_validation: true,
                },
            )
            .unwrap();

        // Invalidate block by return back `tx` with not ready message.
        let index = block.transactions().len() - 1;
        block.transactions_mut().insert(index, tx);
        let res = make_executor(&[&message]).execute_and_commit(
            ExecutionBlock::Validation(block),
            ExecutionOptions {
                utxo_validation: true,
            },
        );
        assert!(matches!(
            res,
            Err(ExecutorError::TransactionValidity(
                TransactionValidityError::MessageSpendTooEarly(_)
            ))
        ));
    }

    #[test]
    fn message_input_fails_when_mismatches_database() {
        let mut rng = StdRng::seed_from_u64(2322);

        let (tx, mut message) = make_tx_and_message(&mut rng, 0);

        // Modifying the message to make it mismatch
        message.set_amount(123);

        let mut block = Block::default();
        *block.transactions_mut() = vec![tx.clone()];

        let ExecutionResult {
            skipped_transactions,
            ..
        } = make_executor(&[&message])
            .execute_and_commit(
                ExecutionBlock::Production(block.clone().into()),
                ExecutionOptions {
                    utxo_validation: true,
                },
            )
            .unwrap();
        let err = &skipped_transactions[0].1;
        assert!(matches!(
            err,
            &ExecutorError::TransactionValidity(
                TransactionValidityError::MessageMismatch(_)
            )
        ));
    }

    #[test]
    fn message_fails_when_spending_already_spent_message_id() {
        let mut rng = StdRng::seed_from_u64(2322);

        // Create two transactions with the same message
        let (tx1, message) = make_tx_and_message(&mut rng, 0);
        let (mut tx2, _) = make_tx_and_message(&mut rng, 0);
        tx2.as_script_mut().unwrap().inputs_mut()[0] =
            tx1.as_script().unwrap().inputs()[0].clone();

        let mut block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx1, tx2.clone()],
        };

        let exec = make_executor(&[&message]);
        let ExecutionData {
            skipped_transactions,
            ..
        } = exec
            .execute_block(
                ExecutionType::Production(PartialBlockComponent::from_partial_block(
                    &mut block,
                )),
                ExecutionOptions {
                    utxo_validation: true,
                },
            )
            .unwrap();
        // One of two transactions is skipped.
        assert_eq!(skipped_transactions.len(), 1);
        let err = &skipped_transactions[0].1;
        assert!(matches!(
            err,
            &ExecutorError::TransactionValidity(
                TransactionValidityError::MessageAlreadySpent(_)
            )
        ));

        // Produced block is valid
        let exec = make_executor(&[&message]);
        exec.execute_block(
            ExecutionType::Validation(PartialBlockComponent::from_partial_block(
                &mut block,
            )),
            ExecutionOptions {
                utxo_validation: true,
            },
        )
        .unwrap();

        // Invalidate block by return back `tx2` transaction skipped during production.
        block.transactions.insert(block.transactions.len() - 1, tx2);
        let exec = make_executor(&[&message]);
        let res = exec.execute_block(
            ExecutionType::Validation(PartialBlockComponent::from_partial_block(
                &mut block,
            )),
            ExecutionOptions {
                utxo_validation: true,
            },
        );
        assert!(matches!(
            res,
            Err(ExecutorError::TransactionValidity(
                TransactionValidityError::MessageAlreadySpent(_)
            ))
        ));
    }

    #[test]
    fn get_block_height_returns_current_executing_block() {
        let mut rng = StdRng::seed_from_u64(1234);

        let base_asset_id = rng.gen();

        // return current block height
        let script = vec![op::bhei(0x10), op::ret(0x10)];
        let tx = TransactionBuilder::script(script.into_iter().collect(), vec![])
            .script_gas_limit(10000)
            .add_unsigned_coin_input(
                SecretKey::random(&mut rng),
                rng.gen(),
                1000,
                base_asset_id,
                Default::default(),
                Default::default(),
            )
            .finalize();

        // setup block
        let block_height = rng.gen_range(5u32..1000u32);
        let block_tx_idx = rng.gen();

        let block = PartialFuelBlock {
            header: PartialBlockHeader {
                consensus: ConsensusHeader {
                    height: block_height.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions: vec![tx.clone().into()],
        };

        // setup db with coin to spend
        let database = &mut &mut Database::default();
        let coin_input = &tx.inputs()[0];
        let mut coin = CompressedCoin::default();
        coin.set_owner(*coin_input.input_owner().unwrap());
        coin.set_amount(coin_input.amount().unwrap());
        coin.set_asset_id(*coin_input.asset_id(&base_asset_id).unwrap());
        coin.set_maturity(coin_input.maturity().unwrap());
        coin.set_tx_pointer(TxPointer::new(Default::default(), block_tx_idx));
        database
            .storage::<Coins>()
            .insert(coin_input.utxo_id().unwrap(), &coin)
            .unwrap();

        // make executor with db
        let executor = create_executor(
            database.clone(),
            Config {
                utxo_validation_default: true,
                ..Default::default()
            },
        );

        let ExecutionResult { tx_status, .. } = executor
            .execute_and_commit(
                ExecutionBlock::Production(block),
                ExecutionOptions {
                    utxo_validation: true,
                },
            )
            .expect("Should execute the block");

        let receipts = tx_status[0].result.receipts();
        assert_eq!(block_height as u64, receipts[0].val().unwrap());
    }

    #[test]
    fn get_time_returns_current_executing_block_time() {
        let mut rng = StdRng::seed_from_u64(1234);

        let base_asset_id = rng.gen();

        // return current block height
        let script = vec![op::bhei(0x10), op::time(0x11, 0x10), op::ret(0x11)];
        let tx = TransactionBuilder::script(script.into_iter().collect(), vec![])
            .script_gas_limit(10000)
            .add_unsigned_coin_input(
                SecretKey::random(&mut rng),
                rng.gen(),
                1000,
                base_asset_id,
                Default::default(),
                Default::default(),
            )
            .finalize();

        // setup block
        let block_height = rng.gen_range(5u32..1000u32);
        let time = Tai64(rng.gen_range(1u32..u32::MAX) as u64);

        let block = PartialFuelBlock {
            header: PartialBlockHeader {
                consensus: ConsensusHeader {
                    height: block_height.into(),
                    time,
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions: vec![tx.clone().into()],
        };

        // setup db with coin to spend
        let database = &mut &mut Database::default();
        let coin_input = &tx.inputs()[0];
        let mut coin = CompressedCoin::default();
        coin.set_owner(*coin_input.input_owner().unwrap());
        coin.set_amount(coin_input.amount().unwrap());
        coin.set_asset_id(*coin_input.asset_id(&base_asset_id).unwrap());
        coin.set_maturity(coin_input.maturity().unwrap());
        database
            .storage::<Coins>()
            .insert(coin_input.utxo_id().unwrap(), &coin)
            .unwrap();

        // make executor with db
        let executor = create_executor(
            database.clone(),
            Config {
                utxo_validation_default: true,
                ..Default::default()
            },
        );

        let ExecutionResult { tx_status, .. } = executor
            .execute_and_commit(
                ExecutionBlock::Production(block),
                ExecutionOptions {
                    utxo_validation: true,
                },
            )
            .expect("Should execute the block");

        let receipts = tx_status[0].result.receipts();
        assert_eq!(time.0, receipts[0].val().unwrap());
    }

    #[cfg(feature = "relayer")]
    mod relayer {
        use super::*;
        use crate::database::database_description::{
            on_chain::OnChain,
            relayer::Relayer,
        };
        use fuel_core_relayer::storage::EventsHistory;
        use fuel_core_storage::{
            tables::{
                FuelBlocks,
                SpentMessages,
            },
            transactional::Transaction,
            StorageAsMut,
        };

        fn database_with_genesis_block(da_block_height: u64) -> Database<OnChain> {
            let db = Database::default();
            let mut block = Block::default();
            block.header_mut().set_da_height(da_block_height.into());
            block.header_mut().recalculate_metadata();

            let mut db_transaction = db.transaction();
            db_transaction
                .as_mut()
                .storage::<FuelBlocks>()
                .insert(&0.into(), &block)
                .expect("Should insert genesis block without any problems");
            db_transaction.commit().expect("Should commit");
            db
        }

        fn add_message_to_relayer(db: &mut Database<Relayer>, message: Message) {
            let mut db_transaction = db.transaction();
            let da_height = message.da_height();
            db.storage::<EventsHistory>()
                .insert(&da_height, &[Event::Message(message)])
                .expect("Should insert event");
            db_transaction.commit().expect("Should commit events");
        }

        fn add_messages_to_relayer(db: &mut Database<Relayer>, relayer_da_height: u64) {
            for da_height in 0..=relayer_da_height {
                let mut message = Message::default();
                message.set_da_height(da_height.into());
                message.set_nonce(da_height.into());

                add_message_to_relayer(db, message);
            }
        }

        fn create_relayer_executor(
            on_chain: Database<OnChain>,
            relayer: Database<Relayer>,
        ) -> Executor<Database<OnChain>, Database<Relayer>> {
            Executor {
                database_view_provider: on_chain,
                relayer_view_provider: relayer,
                config: Arc::new(Default::default()),
            }
        }

        struct Input {
            relayer_da_height: u64,
            block_height: u32,
            block_da_height: u64,
            genesis_da_height: Option<u64>,
        }

        #[test_case::test_case(
            Input {
                relayer_da_height: 10,
                block_height: 1,
                block_da_height: 10,
                genesis_da_height: Some(0),
            } => matches Ok(()) ; "block producer takes all 10 messages from the relayer"
        )]
        #[test_case::test_case(
            Input {
                relayer_da_height: 10,
                block_height: 1,
                block_da_height: 5,
                genesis_da_height: Some(0),
            } => matches Ok(()) ; "block producer takes first 5 messages from the relayer"
        )]
        #[test_case::test_case(
            Input {
                relayer_da_height: 10,
                block_height: 1,
                block_da_height: 10,
                genesis_da_height: Some(5),
            } => matches Ok(()) ; "block producer takes last 5 messages from the relayer"
        )]
        #[test_case::test_case(
            Input {
                relayer_da_height: 10,
                block_height: 1,
                block_da_height: 10,
                genesis_da_height: Some(u64::MAX),
            } => matches Err(ExecutorError::DaHeightExceededItsLimit) ; "block producer fails when previous block exceeds `u64::MAX`"
        )]
        #[test_case::test_case(
            Input {
                relayer_da_height: 10,
                block_height: 1,
                block_da_height: 10,
                genesis_da_height: None,
            } => matches Err(ExecutorError::PreviousBlockIsNotFound) ; "block producer fails when previous block doesn't exist"
        )]
        #[test_case::test_case(
            Input {
                relayer_da_height: 10,
                block_height: 0,
                block_da_height: 10,
                genesis_da_height: Some(0),
            } => matches Err(ExecutorError::ExecutingGenesisBlock) ; "block producer fails when block height is zero"
        )]
        fn block_producer_takes_messages_from_the_relayer(
            input: Input,
        ) -> Result<(), ExecutorError> {
            let genesis_da_height = input.genesis_da_height.unwrap_or_default();
            let on_chain_db = if let Some(genesis_da_height) = input.genesis_da_height {
                database_with_genesis_block(genesis_da_height)
            } else {
                Database::default()
            };
            let mut relayer_db = Database::<Relayer>::default();

            // Given
            let relayer_da_height = input.relayer_da_height;
            let block_height = input.block_height;
            let block_da_height = input.block_da_height;
            add_messages_to_relayer(&mut relayer_db, relayer_da_height);
            assert_eq!(on_chain_db.iter_all::<Messages>(None).count(), 0);

            // When
            let producer = create_relayer_executor(on_chain_db, relayer_db);
            let block = test_block(block_height.into(), block_da_height.into(), 0);
            let result = producer.execute_and_commit(
                ExecutionTypes::Production(block.into()),
                Default::default(),
            )?;

            // Then
            let view = producer.database_view_provider.latest_view();
            assert_eq!(
                view.iter_all::<Messages>(None).count() as u64,
                block_da_height - genesis_da_height
            );
            assert_eq!(
                result.events.len() as u64,
                block_da_height - genesis_da_height
            );
            let messages = view.iter_all::<Messages>(None);
            for ((da_height, message), event) in (genesis_da_height + 1..block_da_height)
                .zip(messages)
                .zip(result.events.iter())
            {
                let (_, message) = message.unwrap();
                assert_eq!(message.da_height(), da_height.into());
                assert!(matches!(event, ExecutorEvent::MessageImported(_)));
            }
            Ok(())
        }

        #[test]
        fn block_producer_does_not_take_messages_for_the_same_height() {
            let genesis_da_height = 1u64;
            let on_chain_db = database_with_genesis_block(genesis_da_height);
            let mut relayer_db = Database::<Relayer>::default();

            // Given
            let relayer_da_height = 10u64;
            let block_height = 1u32;
            let block_da_height = 1u64;
            add_messages_to_relayer(&mut relayer_db, relayer_da_height);
            assert_eq!(on_chain_db.iter_all::<Messages>(None).count(), 0);

            // When
            let producer = create_relayer_executor(on_chain_db, relayer_db);
            let block = test_block(block_height.into(), block_da_height.into(), 10);
            let result = producer
                .execute_and_commit(
                    ExecutionTypes::Production(block.into()),
                    Default::default(),
                )
                .unwrap();

            // Then
            let view = producer.database_view_provider.latest_view();
            assert!(result.skipped_transactions.is_empty());
            assert_eq!(view.iter_all::<Messages>(None).count() as u64, 0);
        }

        #[test]
        fn block_producer_can_use_just_added_message_in_the_transaction() {
            let genesis_da_height = 1u64;
            let on_chain_db = database_with_genesis_block(genesis_da_height);
            let mut relayer_db = Database::<Relayer>::default();

            let block_height = 1u32;
            let block_da_height = 2u64;
            let nonce = 1.into();
            let mut message = Message::default();
            message.set_da_height(block_da_height.into());
            message.set_nonce(nonce);
            add_message_to_relayer(&mut relayer_db, message);

            // Given
            assert_eq!(on_chain_db.iter_all::<Messages>(None).count(), 0);
            assert_eq!(on_chain_db.iter_all::<SpentMessages>(None).count(), 0);
            let tx = TransactionBuilder::script(vec![], vec![])
                .script_gas_limit(10)
                .add_unsigned_message_input(
                    SecretKey::random(&mut StdRng::seed_from_u64(2322)),
                    Default::default(),
                    nonce,
                    Default::default(),
                    vec![],
                )
                .finalize_as_transaction();

            // When
            let mut block = test_block(block_height.into(), block_da_height.into(), 0);
            *block.transactions_mut() = vec![tx];
            let producer = create_relayer_executor(on_chain_db, relayer_db);
            let result = producer
                .execute_and_commit(
                    ExecutionTypes::Production(block.into()),
                    Default::default(),
                )
                .unwrap();

            // Then
            let view = producer.database_view_provider.latest_view();
            assert!(result.skipped_transactions.is_empty());
            assert_eq!(view.iter_all::<Messages>(None).count() as u64, 0);
            // Message added during this block immediately became spent.
            assert_eq!(view.iter_all::<SpentMessages>(None).count(), 1);
            assert_eq!(result.events.len(), 2);
            assert!(matches!(
                result.events[0],
                ExecutorEvent::MessageImported(_)
            ));
            assert!(matches!(
                result.events[1],
                ExecutorEvent::MessageConsumed(_)
            ));
        }
    }
}
