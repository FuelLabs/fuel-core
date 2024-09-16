#[allow(clippy::arithmetic_side_effects)]
#[allow(clippy::cast_possible_truncation)]
#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use crate as fuel_core;
    use fuel_core::database::Database;
    use fuel_core_executor::{
        executor::OnceTransactionsSource,
        ports::{
            MaybeCheckedTransaction,
            RelayerPort,
        },
        refs::ContractRef,
    };
    use fuel_core_storage::{
        tables::{
            Coins,
            ConsensusParametersVersions,
            ContractsRawCode,
            Messages,
        },
        transactional::{
            AtomicView,
            WriteTransaction,
        },
        Result as StorageResult,
        StorageAsMut,
        StorageAsRef,
    };
    use fuel_core_types::{
        blockchain::{
            block::{
                Block,
                PartialFuelBlock,
            },
            header::{
                ApplicationHeader,
                ConsensusHeader,
                PartialBlockHeader,
            },
            primitives::DaBlockHeight,
        },
        entities::{
            coins::coin::CompressedCoin,
            relayer::message::{
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
        fuel_merkle::{
            common::empty_sum_sha256,
            sparse,
        },
        fuel_tx::{
            consensus_parameters::gas::GasCostsValuesV1,
            field::{
                InputContract,
                Inputs,
                MintAmount,
                MintAssetId,
                OutputContract,
                Outputs,
                Policies,
                Script as ScriptField,
                TxPointer as TxPointerTraitTrait,
            },
            input::{
                coin::{
                    CoinPredicate,
                    CoinSigned,
                },
                contract,
                Input,
            },
            policies::PolicyType,
            Bytes32,
            Cacheable,
            ConsensusParameters,
            Create,
            DependentCost,
            FeeParameters,
            Finalizable,
            GasCostsValues,
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
            checked_transaction::{
                CheckError,
                EstimatePredicates,
                IntoChecked,
            },
            interpreter::{
                ExecutableTransaction,
                MemoryInstance,
            },
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
                ExecutionResult,
                TransactionExecutionResult,
                TransactionValidityError,
            },
            relayer::Event,
        },
        tai64::Tai64,
    };
    use fuel_core_upgradable_executor::executor::Executor;
    use itertools::Itertools;
    use rand::{
        prelude::StdRng,
        Rng,
        SeedableRng,
    };

    #[derive(Clone, Debug, Default)]
    struct Config {
        /// Network-wide common parameters used for validating the chain.
        /// The executor already has these parameters, and this field allows us
        /// to override the existing value.
        pub consensus_parameters: ConsensusParameters,
        /// Print execution backtraces if transaction execution reverts.
        pub backtrace: bool,
        /// Default mode for utxo_validation
        pub utxo_validation_default: bool,
    }

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
        type LatestView = Self;

        fn latest_view(&self) -> StorageResult<Self::LatestView> {
            Ok(self.clone())
        }
    }

    fn add_consensus_parameters(
        mut database: Database,
        consensus_parameters: &ConsensusParameters,
    ) -> Database {
        // Set the consensus parameters for the executor.
        let mut tx = database.write_transaction();
        tx.storage_as_mut::<ConsensusParametersVersions>()
            .insert(&0, consensus_parameters)
            .unwrap();
        tx.commit().unwrap();
        database
    }

    fn create_executor(
        database: Database,
        config: Config,
    ) -> Executor<Database, DisabledRelayer> {
        let executor_config = fuel_core_upgradable_executor::config::Config {
            backtrace: config.backtrace,
            utxo_validation_default: config.utxo_validation_default,
            native_executor_version: None,
        };

        let database = add_consensus_parameters(database, &config.consensus_parameters);

        Executor::new(database, DisabledRelayer, executor_config)
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
            .script_gas_limit(TxParameters::DEFAULT.max_gas_per_tx() >> 1)
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
        let transactions = (1..num_txs + 1).map(script_tx_for_amount).collect_vec();

        let mut block = Block::default();
        block.header_mut().set_block_height(block_height);
        block.header_mut().set_da_height(da_block_height);
        *block.transactions_mut() = transactions;
        block
    }

    fn script_tx_for_amount(amount: usize) -> Transaction {
        let asset = AssetId::BASE;
        TxBuilder::new(2322u64)
            .script_gas_limit(10)
            .coin_input(asset, (amount as Word) * 100)
            .coin_output(asset, (amount as Word) * 50)
            .change_output(asset)
            .build()
            .transaction()
            .to_owned()
            .into()
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
        let mut producer = create_executor(Default::default(), Default::default());
        let verifier = create_executor(Default::default(), Default::default());
        let block = test_block(1u32.into(), 0u64.into(), 10);

        let ExecutionResult {
            block,
            skipped_transactions,
            ..
        } = producer.produce_and_commit(block.into()).unwrap();

        let validation_result = verifier.validate(&block);
        assert!(validation_result.is_ok());
        assert!(skipped_transactions.is_empty());
    }

    // Ensure transaction commitment != default after execution
    #[test]
    fn executor_commits_transactions_to_block() {
        let mut producer = create_executor(Default::default(), Default::default());
        let block = test_block(1u32.into(), 0u64.into(), 10);
        let start_block = block.clone();

        let ExecutionResult {
            block,
            skipped_transactions,
            ..
        } = producer.produce_and_commit(block.into()).unwrap();

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
        use crate::graphql_api::ports::DatabaseContracts;

        use super::*;
        use fuel_core_storage::{
            iter::IterDirection,
            transactional::{
                AtomicView,
                Modifiable,
            },
        };
        use fuel_core_types::services::graphql_api::ContractBalance;

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
            let amount = 10000;
            let limit = 0;
            let gas_price_factor = 1;
            let script = TxBuilder::new(1u64)
                .script_gas_limit(limit)
                .max_fee_limit(amount)
                .coin_input(AssetId::BASE, amount)
                .change_output(AssetId::BASE)
                .build()
                .transaction()
                .clone();

            let recipient = Contract::EMPTY_CONTRACT_ID;

            let fee_params =
                FeeParameters::default().with_gas_price_factor(gas_price_factor);
            let mut consensus_parameters = ConsensusParameters::default();
            consensus_parameters.set_fee_params(fee_params);
            let config = Config {
                consensus_parameters: consensus_parameters.clone(),
                ..Default::default()
            };

            let database = &mut Database::default();
            database
                .storage::<ContractsRawCode>()
                .insert(&recipient, &[])
                .expect("Should insert coinbase contract");

            let mut producer = create_executor(database.clone(), config);

            let expected_fee_amount_1 = TransactionFee::checked_from_tx(
                consensus_parameters.gas_costs(),
                consensus_parameters.fee_params(),
                &script,
                price,
            )
            .unwrap()
            .max_fee();
            let invalid_duplicate_tx = script.clone().into();

            let mut header = PartialBlockHeader::default();
            header.consensus.height = 1.into();

            let (
                ExecutionResult {
                    block,
                    skipped_transactions,
                    ..
                },
                changes,
            ) = producer
                .produce_without_commit_with_source(Components {
                    header_to_produce: header,
                    transactions_source: OnceTransactionsSource::new(vec![
                        script.into(),
                        invalid_duplicate_tx,
                    ]),
                    gas_price: price,
                    coinbase_recipient: recipient,
                })
                .unwrap()
                .into();
            producer
                .storage_view_provider
                .commit_changes(changes)
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

            let ContractBalance {
                asset_id, amount, ..
            } = producer
                .storage_view_provider
                .latest_view()
                .unwrap()
                .contract_balances(recipient, None, IterDirection::Forward)
                .next()
                .unwrap()
                .unwrap();
            assert_eq!(asset_id, AssetId::zeroed());
            assert_eq!(amount, expected_fee_amount_1);

            let script = TxBuilder::new(2u64)
                .script_gas_limit(limit)
                .max_fee_limit(amount)
                .coin_input(AssetId::BASE, amount)
                .change_output(AssetId::BASE)
                .build()
                .transaction()
                .clone();

            let expected_fee_amount_2 = TransactionFee::checked_from_tx(
                consensus_parameters.gas_costs(),
                consensus_parameters.fee_params(),
                &script,
                price,
            )
            .unwrap()
            .max_fee();

            let mut header = PartialBlockHeader::default();
            header.consensus.height = 2.into();

            let (
                ExecutionResult {
                    block,
                    skipped_transactions,
                    ..
                },
                changes,
            ) = producer
                .produce_without_commit_with_source(Components {
                    header_to_produce: header,
                    transactions_source: OnceTransactionsSource::new(vec![script.into()]),
                    gas_price: price,
                    coinbase_recipient: recipient,
                })
                .unwrap()
                .into();
            producer
                .storage_view_provider
                .commit_changes(changes)
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
                    UtxoId::new(first_mint.id(&consensus_parameters.chain_id()), 0)
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
            let ContractBalance {
                asset_id, amount, ..
            } = producer
                .storage_view_provider
                .latest_view()
                .unwrap()
                .contract_balances(recipient, None, IterDirection::Forward)
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

            let fee_params =
                FeeParameters::default().with_gas_price_factor(gas_price_factor);
            let mut consensus_parameters = ConsensusParameters::default();
            consensus_parameters.set_fee_params(fee_params);
            let config = Config {
                consensus_parameters,
                ..Default::default()
            };
            let recipient = [1u8; 32].into();

            let producer = create_executor(Default::default(), config);

            let result = producer
                .dry_run_without_commit_with_source(Components {
                    header_to_produce: Default::default(),
                    transactions_source: OnceTransactionsSource::new(vec![script.into()]),
                    coinbase_recipient: recipient,
                    gas_price: 0,
                })
                .unwrap();
            let ExecutionResult { block, .. } = result.into_result();

            assert_eq!(block.transactions().len(), 1);
        }

        #[test]
        fn executor_commits_transactions_with_non_zero_coinbase_validation() {
            let price = 1;
            let amount = 10000;
            let limit = 0;
            let gas_price_factor = 1;
            let script = TxBuilder::new(2322u64)
                .script_gas_limit(limit)
                .max_fee_limit(amount)
                .coin_input(AssetId::BASE, 10000)
                .change_output(AssetId::BASE)
                .build()
                .transaction()
                .clone();
            let recipient = Contract::EMPTY_CONTRACT_ID;

            let fee_params =
                FeeParameters::default().with_gas_price_factor(gas_price_factor);
            let mut consensus_parameters = ConsensusParameters::default();
            consensus_parameters.set_fee_params(fee_params);
            let config = Config {
                consensus_parameters,
                ..Default::default()
            };
            let database = &mut Database::default();

            database
                .storage::<ContractsRawCode>()
                .insert(&recipient, &[])
                .expect("Should insert coinbase contract");

            let producer = create_executor(database.clone(), config.clone());

            let ExecutionResult {
                block,
                skipped_transactions,
                ..
            } = producer
                .produce_without_commit_with_source(Components {
                    header_to_produce: PartialBlockHeader::default(),
                    transactions_source: OnceTransactionsSource::new(vec![script.into()]),
                    gas_price: price,
                    coinbase_recipient: recipient,
                })
                .unwrap()
                .into_result();
            assert!(skipped_transactions.is_empty());
            let produced_txs = block.transactions().to_vec();

            let mut validator = create_executor(
                Default::default(),
                // Use the same config as block producer
                config,
            );
            let _ = validator.validate_and_commit(&block).unwrap();
            assert_eq!(block.transactions(), produced_txs);
            let ContractBalance {
                asset_id, amount, ..
            } = validator
                .storage_view_provider
                .latest_view()
                .unwrap()
                .contract_balances(recipient, None, IterDirection::Forward)
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
                        op::ret(0x13),
                    ], expected_in_tx_coinbase.to_vec() /* pass expected address as script data */)
                    .coin_input(AssetId::BASE, 1000)
                    .variable_output(Default::default())
                    .coin_output(AssetId::BASE, 1000)
                    .change_output(AssetId::BASE)
                    .build()
                    .transaction()
                    .clone();

                let mut producer =
                    create_executor(Default::default(), Default::default());

                let mut block = Block::default();
                *block.transactions_mut() = vec![script.clone().into()];

                let (ExecutionResult { tx_status, .. }, changes) = producer
                    .produce_without_commit_with_coinbase(
                        block.into(),
                        config_coinbase,
                        0,
                    )
                    .expect("Should execute the block")
                    .into();
                producer
                    .storage_view_provider
                    .commit_changes(changes)
                    .unwrap();
                let receipts = tx_status[0].result.receipts();

                if let Some(Receipt::Return { val, .. }) = receipts.first() {
                    *val == 1
                } else {
                    panic!("Execution of the `CB` script failed failed")
                }
            }

            assert!(compare_coinbase_addresses(
                ContractId::from([1u8; 32]),
                ContractId::from([1u8; 32]),
            ));
            assert!(!compare_coinbase_addresses(
                ContractId::from([9u8; 32]),
                ContractId::from([1u8; 32]),
            ));
            assert!(!compare_coinbase_addresses(
                ContractId::from([1u8; 32]),
                ContractId::from([9u8; 32]),
            ));
            assert!(compare_coinbase_addresses(
                ContractId::from([9u8; 32]),
                ContractId::from([9u8; 32]),
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
                Default::default(),
            );

            let mut block = Block::default();
            *block.transactions_mut() = vec![mint.into()];
            block.header_mut().recalculate_metadata();

            let mut validator = create_executor(
                Default::default(),
                Config {
                    utxo_validation_default: false,
                    ..Default::default()
                },
            );
            let validation_err = validator
                .validate_and_commit(&block)
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
                Default::default(),
            );
            let tx = Transaction::default_test_tx();

            let mut block = Block::default();
            *block.transactions_mut() = vec![mint.clone().into(), tx, mint.into()];
            block.header_mut().recalculate_metadata();

            let mut validator = create_executor(Default::default(), Default::default());
            let validation_err = validator
                .validate_and_commit(&block)
                .expect_err("Expected error because coinbase if invalid");
            assert!(matches!(
                validation_err,
                ExecutorError::MintIsNotLastTransaction
            ));
        }

        #[test]
        fn invalidate_block_missed_coinbase() {
            let block = Block::default();

            let mut validator = create_executor(Default::default(), Default::default());
            let validation_err = validator
                .validate_and_commit(&block)
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
                Default::default(),
            );

            let mut block = Block::default();
            *block.transactions_mut() = vec![mint.into()];
            block.header_mut().recalculate_metadata();

            let mut validator = create_executor(Default::default(), Default::default());
            let validation_err = validator
                .validate_and_commit(&block)
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
                Default::default(),
            );

            let mut block = Block::default();
            *block.transactions_mut() = vec![mint.into()];
            block.header_mut().recalculate_metadata();

            let mut consensus_parameters = ConsensusParameters::default();
            consensus_parameters.set_base_asset_id([1u8; 32].into());

            let config = Config {
                consensus_parameters,
                ..Default::default()
            };
            let mut validator = create_executor(Default::default(), config);
            let validation_err = validator
                .validate_and_commit(&block)
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
                Default::default(),
            );

            let mut block = Block::default();
            *block.transactions_mut() = vec![mint.into()];
            block.header_mut().recalculate_metadata();

            let mut validator = create_executor(Default::default(), Default::default());
            let validation_err = validator
                .validate_and_commit(&block)
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
        let consensus_parameters = ConsensusParameters::default();
        let config = Config {
            consensus_parameters: consensus_parameters.clone(),
            ..Default::default()
        };
        let producer = create_executor(Default::default(), config.clone());

        let verifier = create_executor(Default::default(), config);

        let gas_limit = 100;
        let max_fee = 1;
        let script = TransactionBuilder::script(vec![], vec![])
            .add_unsigned_coin_input(
                SecretKey::random(&mut rng),
                rng.gen(),
                rng.gen(),
                rng.gen(),
                Default::default(),
            )
            .script_gas_limit(gas_limit)
            .max_fee_limit(max_fee)
            .finalize();
        let tx: Transaction = script.into();

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx.clone()],
        };

        let ExecutionResult {
            skipped_transactions,
            mut block,
            ..
        } = producer
            .produce_without_commit(block)
            .unwrap()
            .into_result();
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
        let _ = verifier.validate(&block).unwrap().into_result();

        // Invalidate the block with Insufficient tx
        let len = block.transactions().len();
        block.transactions_mut().insert(len - 1, tx);
        let verify_result = verifier.validate(&block);
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

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![
                Transaction::default_test_tx(),
                Transaction::default_test_tx(),
            ],
        };

        let ExecutionResult {
            skipped_transactions,
            mut block,
            ..
        } = producer
            .produce_without_commit(block)
            .unwrap()
            .into_result();
        let produce_result = &skipped_transactions[0].1;
        assert!(matches!(
            produce_result,
            &ExecutorError::TransactionIdCollision(_)
        ));

        // Produced block is valid
        let _ = verifier.validate(&block).unwrap().into_result();

        // Make the block invalid by adding of the duplicating transaction
        let len = block.transactions().len();
        block
            .transactions_mut()
            .insert(len - 1, Transaction::default_test_tx());
        let verify_result = verifier.validate(&block);
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

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx.clone()],
        };

        let ExecutionResult {
            skipped_transactions,
            mut block,
            ..
        } = producer
            .produce_without_commit(block)
            .unwrap()
            .into_result();
        let produce_result = &skipped_transactions[0].1;
        assert!(matches!(
            produce_result,
            &ExecutorError::TransactionValidity(
                TransactionValidityError::CoinDoesNotExist(_)
            )
        ));

        // Produced block is valid
        let _ = verifier.validate(&block).unwrap().into_result();

        // Invalidate block by adding transaction with not existing coin
        let len = block.transactions().len();
        block.transactions_mut().insert(len - 1, tx);
        let verify_result = verifier.validate(&block);
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
        let chain_id = ConsensusParameters::default().chain_id();
        let transaction_id = tx.id(&chain_id);

        let mut producer = create_executor(Default::default(), Default::default());

        let mut verifier = create_executor(Default::default(), Default::default());

        let mut block = Block::default();
        *block.transactions_mut() = vec![tx];

        let ExecutionResult { mut block, .. } =
            producer.produce_and_commit(block.into()).unwrap();

        // modify change amount
        if let Transaction::Script(script) = &mut block.transactions_mut()[0] {
            if let Output::Change { amount, .. } = &mut script.outputs_mut()[0] {
                *amount = fake_output_amount
            }
        }

        // then
        let err = verifier.validate_and_commit(&block).unwrap_err();
        assert_eq!(
            err,
            ExecutorError::InvalidTransactionOutcome { transaction_id }
        );
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

        let mut producer = create_executor(Default::default(), Default::default());

        let mut verifier = create_executor(Default::default(), Default::default());

        let mut block = Block::default();
        *block.transactions_mut() = vec![tx];

        let ExecutionResult { mut block, .. } =
            producer.produce_and_commit(block.into()).unwrap();

        // randomize transaction commitment
        block.header_mut().set_transaction_root(rng.gen());
        block.header_mut().recalculate_metadata();

        let err = verifier.validate_and_commit(&block).unwrap_err();

        assert_eq!(err, ExecutorError::BlockMismatch)
    }

    // invalidate a block if a tx is missing at least one coin input
    #[test]
    fn executor_invalidates_missing_coin_input() {
        let mut tx: Script = Script::default();
        tx.policies_mut().set(PolicyType::MaxFee, Some(0));

        let mut executor = create_executor(
            Database::default(),
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
        } = executor.produce_and_commit(block).unwrap();

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
        let mut executor = create_executor(
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
        } = executor.produce_and_commit(block).unwrap();
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
        let mut executor = create_executor(
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
        } = executor.produce_and_commit(block).unwrap();
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
        let mut executor = create_executor(
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
        } = executor.produce_and_commit(block).unwrap();
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
    fn transaction_consuming_too_much_gas_are_skipped() {
        // Gather the gas consumption of the transaction
        let mut executor = create_executor(Default::default(), Default::default());
        let block: PartialFuelBlock = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![TransactionBuilder::script(vec![], vec![])
                .max_fee_limit(100_000_000)
                .add_random_fee_input()
                .script_gas_limit(0)
                .tip(123)
                .finalize_as_transaction()],
        };

        // When
        let ExecutionResult { tx_status, .. } =
            executor.produce_and_commit(block).unwrap();
        let tx_gas_usage = tx_status[0].result.total_gas();

        // Given
        let mut txs = vec![];
        for i in 0..10 {
            let tx = TransactionBuilder::script(vec![], vec![])
                .max_fee_limit(100_000_000)
                .add_random_fee_input()
                .script_gas_limit(0)
                .tip(i * 100)
                .finalize_as_transaction();
            txs.push(tx);
        }
        let mut config: Config = Default::default();
        // Each TX consumes `tx_gas_usage` gas and so set the block gas limit to execute only 9 transactions.
        config
            .consensus_parameters
            .set_block_gas_limit(tx_gas_usage * 9);
        let mut executor = create_executor(Default::default(), config);

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: txs,
        };

        // When
        let ExecutionResult {
            skipped_transactions,
            ..
        } = executor.produce_and_commit(block).unwrap();

        // Then
        assert_eq!(skipped_transactions.len(), 1);
        assert_eq!(skipped_transactions[0].1, ExecutorError::GasOverflow);
    }

    #[test]
    fn skipped_txs_not_affect_order() {
        // `tx1` is invalid because it doesn't have inputs for max fee.
        // `tx2` is a `Create` transaction with some code inside.
        // `tx3` is a `Script` transaction that depends on `tx2`. It will be skipped
        // if `tx2` is not executed before `tx3`.
        //
        // The test checks that execution for the block with transactions [tx1, tx2, tx3] skips
        // transaction `tx1` and produce a block [tx2, tx3] with the expected order.
        let tx1 = TransactionBuilder::script(vec![], vec![])
            .add_random_fee_input()
            .script_gas_limit(1000000)
            .tip(1000000)
            .finalize_as_transaction();
        let (tx2, tx3) = setup_executable_script();

        let mut executor = create_executor(Default::default(), Default::default());

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx1.clone(), tx2.clone().into(), tx3.clone().into()],
        };

        let ExecutionResult {
            block,
            skipped_transactions,
            ..
        } = executor.produce_and_commit(block).unwrap();
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

        let db = &Database::default();
        let mut executor = create_executor(db.clone(), Default::default());

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        let ExecutionResult { block, .. } = executor.produce_and_commit(block).unwrap();

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

        let mut executor = create_executor(
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
        } = executor.produce_and_commit(block).unwrap();

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

        let mut executor = create_executor(
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
        } = executor.produce_and_commit(block).unwrap();

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
        let db = Database::default();

        let mut executor = create_executor(
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
        } = executor.produce_and_commit(block).unwrap();

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
        let db = Database::default();

        let consensus_parameters = ConsensusParameters::default();
        let mut executor = create_executor(
            db.clone(),
            Config {
                utxo_validation_default: false,
                consensus_parameters: consensus_parameters.clone(),
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

        let ExecutionResult { block, .. } = executor.produce_and_commit(block).unwrap();

        let executed_tx = block.transactions()[1].as_script().unwrap();
        let state_root = executed_tx.outputs()[0].state_root();
        let balance_root = executed_tx.outputs()[0].balance_root();

        let mut new_tx = executed_tx.clone();
        *new_tx.script_mut() = vec![];
        new_tx.precompute(&consensus_parameters.chain_id()).unwrap();

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
            .produce_without_commit_with_source(Components {
                header_to_produce: block.header,
                transactions_source: OnceTransactionsSource::new(block.transactions),
                gas_price: 0,
                coinbase_recipient: Default::default(),
            })
            .unwrap()
            .into_result();
        assert!(matches!(
            tx_status[1].result,
            TransactionExecutionResult::Success { .. }
        ));
        let tx = block.transactions()[0].as_script().unwrap();
        assert_eq!(tx.inputs()[0].balance_root(), balance_root);
        assert_eq!(tx.inputs()[0].state_root(), state_root);

        let _ = executor
            .validate(&block)
            .expect("Validation of block should be successful");
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

        let mut executor = create_executor(db.clone(), Default::default());

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

        let _ = executor.produce_and_commit(block).unwrap();

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

        let mut executor = create_executor(
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

        let ExecutionResult { block, events, .. } =
            executor.produce_and_commit(block).unwrap();

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

        let mut setup = create_executor(db.clone(), Default::default());

        let ExecutionResult {
            skipped_transactions,
            ..
        } = setup.produce_and_commit(first_block).unwrap();
        assert!(skipped_transactions.is_empty());

        let producer = create_executor(db.clone(), Default::default());
        let ExecutionResult {
            block: second_block,
            skipped_transactions,
            ..
        } = producer
            .produce_without_commit(second_block)
            .unwrap()
            .into_result();
        assert!(skipped_transactions.is_empty());

        let verifier = create_executor(db, Default::default());
        let verify_result = verifier.validate(&second_block);
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

        let mut setup = create_executor(db.clone(), Default::default());

        setup.produce_and_commit(first_block).unwrap();

        let producer = create_executor(db.clone(), Default::default());

        let ExecutionResult {
            block: mut second_block,
            ..
        } = producer
            .produce_without_commit(second_block)
            .unwrap()
            .into_result();
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
        let err = verifier.validate(&second_block).unwrap_err();

        assert_eq!(
            err,
            ExecutorError::InvalidTransactionOutcome {
                transaction_id: tx_id
            }
        );
    }

    #[test]
    fn outputs_with_amount_are_included_utxo_set() {
        let (deploy, script) = setup_executable_script();
        let script_id = script.id(&ChainId::default());

        let database = &Database::default();
        let mut executor = create_executor(database.clone(), Default::default());

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![deploy.into(), script.into()],
        };

        let ExecutionResult { block, .. } = executor.produce_and_commit(block).unwrap();

        // ensure that all utxos with an amount are stored into the utxo set
        for (idx, output) in block.transactions()[1]
            .as_script()
            .unwrap()
            .outputs()
            .iter()
            .enumerate()
        {
            let id = UtxoId::new(script_id, idx as u16);
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

        let database = &Database::default();
        let mut executor = create_executor(database.clone(), Default::default());

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        executor.produce_and_commit(block).unwrap();

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
            .add_output(Output::change(rng.gen(), 1000, AssetId::BASE))
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
            .produce_and_commit(block)
            .expect("block execution failed unexpectedly");

        make_executor(&[&message])
            .validate_and_commit(&block)
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

        let mut exec = make_executor(&messages);
        let view = exec.storage_view_provider.latest_view().unwrap();
        assert!(view.message_exists(message_coin.nonce()).unwrap());
        assert!(view.message_exists(message_data.nonce()).unwrap());

        let ExecutionResult {
            skipped_transactions,
            ..
        } = exec.produce_and_commit(block).unwrap();
        assert_eq!(skipped_transactions.len(), 0);

        // Successful execution consumes `message_coin` and `message_data`.
        let view = exec.storage_view_provider.latest_view().unwrap();
        assert!(!view.message_exists(message_coin.nonce()).unwrap());
        assert!(!view.message_exists(message_data.nonce()).unwrap());
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

        let mut exec = make_executor(&messages);
        let view = exec.storage_view_provider.latest_view().unwrap();
        assert!(view.message_exists(message_coin.nonce()).unwrap());
        assert!(view.message_exists(message_data.nonce()).unwrap());

        let ExecutionResult {
            skipped_transactions,
            ..
        } = exec.produce_and_commit(block).unwrap();
        assert_eq!(skipped_transactions.len(), 0);

        // We should spend only `message_coin`. The `message_data` should be unspent.
        let view = exec.storage_view_provider.latest_view().unwrap();
        assert!(!view.message_exists(message_coin.nonce()).unwrap());
        assert!(view.message_exists(message_data.nonce()).unwrap());
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
            .produce_and_commit(block.clone().into())
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
            .validate_and_commit(&block)
            .unwrap();

        // Invalidate block by returning back `tx` with not existing message
        let index = block.transactions().len() - 1;
        block.transactions_mut().insert(index, tx);
        let res = make_executor(&[]) // No messages in the db
            .validate_and_commit(&block);
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
            .produce_and_commit(block.clone().into())
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
            .validate_and_commit(&block)
            .unwrap();

        // Invalidate block by return back `tx` with not ready message.
        let index = block.transactions().len() - 1;
        block.transactions_mut().insert(index, tx);
        let res = make_executor(&[&message]).validate_and_commit(&block);
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
            .produce_and_commit(block.clone().into())
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

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx1, tx2.clone()],
        };

        let exec = make_executor(&[&message]);
        let ExecutionResult {
            skipped_transactions,
            mut block,
            ..
        } = exec.produce_without_commit(block).unwrap().into_result();
        // One of two transactions is skipped.
        assert_eq!(skipped_transactions.len(), 1);
        let err = &skipped_transactions[0].1;
        dbg!(err);
        assert!(matches!(
            err,
            &ExecutorError::TransactionValidity(
                TransactionValidityError::MessageDoesNotExist(_)
            )
        ));

        // Produced block is valid
        let exec = make_executor(&[&message]);
        let _ = exec.validate(&block).unwrap().into_result();

        // Invalidate block by return back `tx2` transaction skipped during production.
        let len = block.transactions().len();
        block.transactions_mut().insert(len - 1, tx2);
        let exec = make_executor(&[&message]);
        let res = exec.validate(&block);
        assert!(matches!(
            res,
            Err(ExecutorError::TransactionValidity(
                TransactionValidityError::MessageDoesNotExist(_)
            ))
        ));
    }

    #[test]
    fn withdrawal_message_included_in_header_for_successfully_executed_transaction() {
        // Given
        let amount_from_random_input = 1000;
        let smo_tx = TransactionBuilder::script(
            vec![
                // The amount to send in coins.
                op::movi(0x13, amount_from_random_input),
                // Send the message output.
                op::smo(0x0, 0x0, 0x0, 0x13),
                op::ret(RegId::ONE),
            ]
            .into_iter()
            .collect(),
            vec![],
        )
        .add_random_fee_input()
        .script_gas_limit(1000000)
        .finalize_as_transaction();

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![smo_tx],
        };

        // When
        let ExecutionResult { block, .. } =
            create_executor(Default::default(), Default::default())
                .produce_and_commit(block)
                .expect("block execution failed unexpectedly");
        let result = create_executor(Default::default(), Default::default())
            .validate_and_commit(&block)
            .expect("block validation failed unexpectedly");

        // Then
        let Some(Receipt::MessageOut {
            sender,
            recipient,
            amount,
            nonce,
            data,
            ..
        }) = result.tx_status[0].result.receipts().first().cloned()
        else {
            panic!("Expected a MessageOut receipt");
        };

        // Reconstruct merkle message outbox merkle root  and see that it matches
        let mut mt = fuel_core_types::fuel_merkle::binary::in_memory::MerkleTree::new();
        mt.push(
            &Message::V1(MessageV1 {
                sender,
                recipient,
                nonce,
                amount,
                data: data.unwrap_or_default(),
                da_height: 1u64.into(),
            })
            .message_id()
            .to_bytes(),
        );
        assert_eq!(block.header().message_outbox_root.as_ref(), mt.root());
    }

    #[test]
    fn withdrawal_message_not_included_in_header_for_failed_transaction() {
        // Given
        let amount_from_random_input = 1000;
        let smo_tx = TransactionBuilder::script(
            vec![
                // The amount to send in coins.
                op::movi(0x13, amount_from_random_input),
                // Send the message output.
                op::smo(0x0, 0x0, 0x0, 0x13),
                op::rvrt(0x0),
            ]
            .into_iter()
            .collect(),
            vec![],
        )
        .add_random_fee_input()
        .script_gas_limit(1000000)
        .finalize_as_transaction();

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![smo_tx],
        };

        // When
        let ExecutionResult { block, .. } =
            create_executor(Default::default(), Default::default())
                .produce_and_commit(block)
                .expect("block execution failed unexpectedly");
        create_executor(Default::default(), Default::default())
            .validate_and_commit(&block)
            .expect("block validation failed unexpectedly");

        // Then
        let empty_root = empty_sum_sha256();
        assert_eq!(block.header().message_outbox_root.as_ref(), empty_root)
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
        coin.set_tx_pointer(TxPointer::new(Default::default(), block_tx_idx));
        database
            .storage::<Coins>()
            .insert(coin_input.utxo_id().unwrap(), &coin)
            .unwrap();

        // make executor with db
        let mut executor = create_executor(
            database.clone(),
            Config {
                utxo_validation_default: true,
                ..Default::default()
            },
        );

        let ExecutionResult { tx_status, .. } = executor
            .produce_and_commit(block)
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
        database
            .storage::<Coins>()
            .insert(coin_input.utxo_id().unwrap(), &coin)
            .unwrap();

        // make executor with db
        let mut executor = create_executor(
            database.clone(),
            Config {
                utxo_validation_default: true,
                ..Default::default()
            },
        );

        let ExecutionResult { tx_status, .. } = executor
            .produce_and_commit(block)
            .expect("Should execute the block");

        let receipts = tx_status[0].result.receipts();
        assert_eq!(time.0, receipts[0].val().unwrap());
    }

    #[test]
    fn tx_with_coin_predicate_included_by_block_producer_and_accepted_by_validator() {
        let mut rng = StdRng::seed_from_u64(2322u64);
        let predicate: Vec<u8> = vec![op::ret(RegId::ONE)].into_iter().collect();
        let owner = Input::predicate_owner(&predicate);
        let amount = 1000;

        let consensus_parameters = ConsensusParameters::default();
        let config = Config {
            utxo_validation_default: true,
            consensus_parameters: consensus_parameters.clone(),
            ..Default::default()
        };

        let mut tx = TransactionBuilder::script(
            vec![op::ret(RegId::ONE)].into_iter().collect(),
            vec![],
        )
        .max_fee_limit(amount)
        .add_input(Input::coin_predicate(
            rng.gen(),
            owner,
            amount,
            AssetId::BASE,
            rng.gen(),
            0,
            predicate,
            vec![],
        ))
        .add_output(Output::Change {
            to: Default::default(),
            amount: 0,
            asset_id: Default::default(),
        })
        .finalize();
        tx.estimate_predicates(
            &consensus_parameters.clone().into(),
            MemoryInstance::new(),
        )
        .unwrap();
        let db = &mut Database::default();

        // insert coin into state
        if let Input::CoinPredicate(CoinPredicate {
            utxo_id,
            owner,
            amount,
            asset_id,
            tx_pointer,
            ..
        }) = tx.inputs()[0]
        {
            let mut coin = CompressedCoin::default();
            coin.set_owner(owner);
            coin.set_amount(amount);
            coin.set_asset_id(asset_id);
            coin.set_tx_pointer(tx_pointer);
            db.storage::<Coins>().insert(&utxo_id, &coin).unwrap();
        }

        let producer = create_executor(db.clone(), config.clone());

        let ExecutionResult {
            block,
            skipped_transactions,
            ..
        } = producer
            .produce_without_commit_with_source(Components {
                header_to_produce: PartialBlockHeader::default(),
                transactions_source: OnceTransactionsSource::new(vec![tx.into()]),
                coinbase_recipient: Default::default(),
                gas_price: 1,
            })
            .unwrap()
            .into_result();
        assert!(skipped_transactions.is_empty());

        let validator = create_executor(db.clone(), config);
        let result = validator.validate(&block);
        assert!(result.is_ok(), "{result:?}")
    }

    #[test]
    fn verifying_during_production_consensus_parameters_version_works() {
        let mut rng = StdRng::seed_from_u64(2322u64);
        let predicate: Vec<u8> = vec![op::ret(RegId::ONE)].into_iter().collect();
        let owner = Input::predicate_owner(&predicate);
        let amount = 1000;
        let cheap_consensus_parameters = ConsensusParameters::default();

        let mut tx = TransactionBuilder::script(vec![], vec![])
            .max_fee_limit(amount)
            .add_input(Input::coin_predicate(
                rng.gen(),
                owner,
                amount,
                AssetId::BASE,
                rng.gen(),
                0,
                predicate,
                vec![],
            ))
            .finalize();
        tx.estimate_predicates(
            &cheap_consensus_parameters.clone().into(),
            MemoryInstance::new(),
        )
        .unwrap();

        // Given
        let gas_costs: GasCostsValues = GasCostsValuesV1 {
            vm_initialization: DependentCost::HeavyOperation {
                base: u32::MAX as u64,
                gas_per_unit: 0,
            },
            ..GasCostsValuesV1::free()
        }
        .into();
        let expensive_consensus_parameters_version = 0;
        let mut expensive_consensus_parameters = ConsensusParameters::default();
        expensive_consensus_parameters.set_gas_costs(gas_costs.into());
        // The block gas limit should cover `vm_initialization` cost
        expensive_consensus_parameters.set_block_gas_limit(u64::MAX);
        let config = Config {
            consensus_parameters: expensive_consensus_parameters.clone(),
            ..Default::default()
        };
        let producer = create_executor(Database::default(), config.clone());

        let cheap_consensus_parameters_version = 1;
        let cheaply_checked_tx = MaybeCheckedTransaction::CheckedTransaction(
            tx.into_checked_basic(0u32.into(), &cheap_consensus_parameters)
                .unwrap()
                .into(),
            cheap_consensus_parameters_version,
        );

        // When
        let ExecutionResult {
            skipped_transactions,
            ..
        } = producer
            .produce_without_commit_with_source(Components {
                header_to_produce: PartialBlockHeader {
                    application: ApplicationHeader {
                        consensus_parameters_version:
                            expensive_consensus_parameters_version,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                transactions_source: OnceTransactionsSource::new_maybe_checked(vec![
                    cheaply_checked_tx,
                ]),
                coinbase_recipient: Default::default(),
                gas_price: 1,
            })
            .unwrap()
            .into_result();

        // Then
        assert_eq!(skipped_transactions.len(), 1);
        assert!(matches!(
            skipped_transactions[0].1,
            ExecutorError::InvalidTransaction(_)
        ));
    }

    #[cfg(feature = "relayer")]
    mod relayer {
        use super::*;
        use crate::{
            database::database_description::{
                on_chain::OnChain,
                relayer::Relayer,
            },
            state::ChangesIterator,
        };
        use fuel_core_relayer::storage::EventsHistory;
        use fuel_core_storage::{
            iter::IteratorOverTable,
            tables::FuelBlocks,
            StorageAsMut,
        };
        use fuel_core_types::{
            entities::RelayedTransaction,
            fuel_merkle::binary::root_calculator::MerkleRootCalculator,
            fuel_tx::{
                output,
                Chargeable,
            },
            services::executor::ForcedTransactionFailure,
        };

        fn database_with_genesis_block(da_block_height: u64) -> Database<OnChain> {
            let mut db = add_consensus_parameters(
                Database::default(),
                &ConsensusParameters::default(),
            );
            let mut block = Block::default();
            block.header_mut().set_da_height(da_block_height.into());
            block.header_mut().recalculate_metadata();

            db.storage_as_mut::<FuelBlocks>()
                .insert(&0.into(), &block)
                .expect("Should insert genesis block without any problems");
            db
        }

        fn add_message_to_relayer(db: &mut Database<Relayer>, message: Message) {
            let da_height = message.da_height();
            db.storage::<EventsHistory>()
                .insert(&da_height, &[Event::Message(message)])
                .expect("Should insert event");
        }

        fn add_events_to_relayer(
            db: &mut Database<Relayer>,
            da_height: DaBlockHeight,
            events: &[Event],
        ) {
            db.storage::<EventsHistory>()
                .insert(&da_height, events)
                .expect("Should insert event");
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
            Executor::new(on_chain, relayer, Default::default())
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
            } => matches Ok(()); "block producer takes all 10 messages from the relayer"
        )]
        #[test_case::test_case(
            Input {
                relayer_da_height: 10,
                block_height: 1,
                block_da_height: 5,
                genesis_da_height: Some(0),
            } => matches Ok(()); "block producer takes first 5 messages from the relayer"
        )]
        #[test_case::test_case(
            Input {
                relayer_da_height: 10,
                block_height: 1,
                block_da_height: 10,
                genesis_da_height: Some(5),
            } => matches Ok(()); "block producer takes last 5 messages from the relayer"
        )]
        #[test_case::test_case(
            Input {
                relayer_da_height: 10,
                block_height: 1,
                block_da_height: 10,
                genesis_da_height: Some(u64::MAX),
            } => matches Err(ExecutorError::DaHeightExceededItsLimit); "block producer fails when previous block exceeds `u64::MAX`"
        )]
        #[test_case::test_case(
            Input {
                relayer_da_height: 10,
                block_height: 1,
                block_da_height: 10,
                genesis_da_height: None,
            } => matches Err(ExecutorError::PreviousBlockIsNotFound); "block producer fails when previous block doesn't exist"
        )]
        #[test_case::test_case(
            Input {
                relayer_da_height: 10,
                block_height: 0,
                block_da_height: 10,
                genesis_da_height: Some(0),
            } => matches Err(ExecutorError::ExecutingGenesisBlock); "block producer fails when block height is zero"
        )]
        fn block_producer_takes_messages_from_the_relayer(
            input: Input,
        ) -> Result<(), ExecutorError> {
            let genesis_da_height = input.genesis_da_height.unwrap_or_default();
            let on_chain_db = if let Some(genesis_da_height) = input.genesis_da_height {
                database_with_genesis_block(genesis_da_height)
            } else {
                add_consensus_parameters(
                    Database::default(),
                    &ConsensusParameters::default(),
                )
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
            let (result, changes) = producer.produce_without_commit(block.into())?.into();

            // Then
            let view = ChangesIterator::<OnChain>::new(&changes);
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
        fn execute_without_commit__block_producer_includes_correct_inbox_event_merkle_root(
        ) {
            // given
            let genesis_da_height = 3u64;
            let on_chain_db = database_with_genesis_block(genesis_da_height);
            let mut relayer_db = Database::<Relayer>::default();
            let block_height = 1u32;
            let relayer_da_height = 10u64;
            let mut root_calculator = MerkleRootCalculator::new();
            for da_height in (genesis_da_height + 1)..=relayer_da_height {
                // message
                let mut message = Message::default();
                message.set_da_height(da_height.into());
                message.set_nonce(da_height.into());
                root_calculator.push(message.message_id().as_ref());
                // transaction
                let mut transaction = RelayedTransaction::default();
                transaction.set_nonce(da_height.into());
                transaction.set_da_height(da_height.into());
                transaction.set_max_gas(da_height);
                transaction.set_serialized_transaction(da_height.to_be_bytes().to_vec());
                root_calculator.push(Bytes32::from(transaction.id()).as_ref());
                // add events to relayer
                add_events_to_relayer(
                    &mut relayer_db,
                    da_height.into(),
                    &[message.into(), transaction.into()],
                );
            }
            let producer = create_relayer_executor(on_chain_db, relayer_db);
            let block = test_block(block_height.into(), relayer_da_height.into(), 0);

            // when
            let (result, _) = producer
                .produce_without_commit(block.into())
                .unwrap()
                .into();

            // then
            let expected = root_calculator.root().into();
            let actual = result.block.header().application().event_inbox_root;
            assert_eq!(actual, expected);
        }

        #[test]
        fn execute_without_commit__relayed_tx_included_in_block() {
            let genesis_da_height = 3u64;
            let block_height = 1u32;
            let da_height = 10u64;
            let arb_large_max_gas = 10_000;

            // given
            let relayer_db =
                relayer_db_with_valid_relayed_txs(da_height, arb_large_max_gas);

            // when
            let on_chain_db = database_with_genesis_block(genesis_da_height);
            let producer = create_relayer_executor(on_chain_db, relayer_db);
            let block = test_block(block_height.into(), da_height.into(), 0);
            let (result, _) = producer
                .produce_without_commit(block.into())
                .unwrap()
                .into();

            // then
            let txs = result.block.transactions();
            assert_eq!(txs.len(), 2);
        }

        fn relayer_db_with_valid_relayed_txs(
            da_height: u64,
            max_gas: u64,
        ) -> Database<Relayer> {
            let mut relayed_tx = RelayedTransaction::default();
            let tx = script_tx_for_amount(100);
            let tx_bytes = tx.to_bytes();
            relayed_tx.set_serialized_transaction(tx_bytes);
            relayed_tx.set_max_gas(max_gas);

            relayer_db_for_events(&[relayed_tx.into()], da_height)
        }

        #[test]
        fn execute_without_commit_with_coinbase__relayed_tx_execute_and_mint_will_have_no_fees(
        ) {
            let genesis_da_height = 3u64;
            let block_height = 1u32;
            let da_height = 10u64;
            let gas_price = 1;
            let arb_max_gas = 10_000;

            // given
            let relayer_db = relayer_db_with_valid_relayed_txs(da_height, arb_max_gas);

            // when
            let on_chain_db = database_with_genesis_block(genesis_da_height);
            let producer = create_relayer_executor(on_chain_db, relayer_db);
            let block = test_block(block_height.into(), da_height.into(), 0);
            let (result, _) = producer
                .produce_without_commit_with_coinbase(
                    block.into(),
                    Default::default(),
                    gas_price,
                )
                .unwrap()
                .into();

            // then
            let txs = result.block.transactions();
            assert_eq!(txs.len(), 2);

            // and
            let mint = txs[1].as_mint().unwrap();
            assert_eq!(*mint.mint_amount(), 0);
        }

        #[test]
        fn execute_without_commit__duplicated_relayed_tx_not_included_in_block() {
            let genesis_da_height = 3u64;
            let block_height = 1u32;
            let da_height = 10u64;
            let duplicate_count = 10;
            let arb_large_max_gas = 10_000;

            // given
            let relayer_db = relayer_db_with_duplicate_valid_relayed_txs(
                da_height,
                duplicate_count,
                arb_large_max_gas,
            );

            // when
            let on_chain_db = database_with_genesis_block(genesis_da_height);
            let producer = create_relayer_executor(on_chain_db, relayer_db);
            let block = test_block(block_height.into(), da_height.into(), 0);
            let (result, _) = producer
                .produce_without_commit(block.into())
                .unwrap()
                .into();

            // then
            let txs = result.block.transactions();
            assert_eq!(txs.len(), 2);

            // and
            let events = result.events;
            let count = events
                .into_iter()
                .filter(|event| {
                    matches!(event, ExecutorEvent::ForcedTransactionFailed { .. })
                })
                .count();
            assert_eq!(count, 10);
        }

        fn relayer_db_with_duplicate_valid_relayed_txs(
            da_height: u64,
            duplicate_count: usize,
            max_gas: u64,
        ) -> Database<Relayer> {
            let mut relayed_tx = RelayedTransaction::default();
            let tx = script_tx_for_amount(100);
            let tx_bytes = tx.to_bytes();
            relayed_tx.set_serialized_transaction(tx_bytes);
            relayed_tx.set_max_gas(max_gas);
            let events = std::iter::repeat(relayed_tx.into())
                .take(duplicate_count + 1)
                .collect::<Vec<_>>();

            relayer_db_for_events(&events, da_height)
        }

        #[test]
        fn execute_without_commit__invalid_relayed_txs_are_not_included_and_are_reported()
        {
            let genesis_da_height = 3u64;
            let block_height = 1u32;
            let da_height = 10u64;
            let arb_large_max_gas = 10_000;

            // given
            let relayer_db =
                relayer_db_with_invalid_relayed_txs(da_height, arb_large_max_gas);

            // when
            let on_chain_db = database_with_genesis_block(genesis_da_height);
            let producer = create_relayer_executor(on_chain_db, relayer_db);
            let block = test_block(block_height.into(), da_height.into(), 0);
            let (result, _) = producer
                .produce_without_commit(block.into())
                .unwrap()
                .into();

            // then
            let txs = result.block.transactions();
            assert_eq!(txs.len(), 1);

            // and
            let events = result.events;
            let fuel_core_types::services::executor::Event::ForcedTransactionFailed {
                failure: actual,
                ..
            } = &events[0]
            else {
                panic!("Expected `ForcedTransactionFailed` event")
            };
            let expected = &ForcedTransactionFailure::CheckError(CheckError::Validity(
                ValidityError::NoSpendableInput,
            ))
            .to_string();
            assert_eq!(expected, actual);
        }

        fn relayer_db_with_invalid_relayed_txs(
            da_height: u64,
            max_gas: u64,
        ) -> Database<Relayer> {
            let event = arb_invalid_relayed_tx_event(max_gas);
            relayer_db_for_events(&[event], da_height)
        }

        #[test]
        fn execute_without_commit__relayed_tx_with_low_max_gas_fails() {
            let genesis_da_height = 3u64;
            let block_height = 1u32;
            let da_height = 10u64;
            let zero_max_gas = 0;

            // given
            let tx = script_tx_for_amount(100);

            let relayer_db = relayer_db_with_specific_tx_for_relayed_tx(
                da_height,
                tx.clone(),
                zero_max_gas,
            );

            // when
            let on_chain_db = database_with_genesis_block(genesis_da_height);
            let producer = create_relayer_executor(on_chain_db, relayer_db);
            let block = test_block(block_height.into(), da_height.into(), 0);
            let (result, _) = producer
                .produce_without_commit(block.into())
                .unwrap()
                .into();

            // then
            let txs = result.block.transactions();
            assert_eq!(txs.len(), 1);

            // and
            let consensus_params = ConsensusParameters::default();
            let actual_max_gas = tx
                .as_script()
                .unwrap()
                .max_gas(consensus_params.gas_costs(), consensus_params.fee_params());
            let events = result.events;
            let fuel_core_types::services::executor::Event::ForcedTransactionFailed {
                failure: actual,
                ..
            } = &events[0]
            else {
                panic!("Expected `ForcedTransactionFailed` event")
            };
            let expected = &ForcedTransactionFailure::InsufficientMaxGas {
                claimed_max_gas: zero_max_gas,
                actual_max_gas,
            }
            .to_string();
            assert_eq!(expected, actual);
        }

        fn relayer_db_with_specific_tx_for_relayed_tx(
            da_height: u64,
            tx: Transaction,
            max_gas: u64,
        ) -> Database<Relayer> {
            let mut relayed_tx = RelayedTransaction::default();
            let tx_bytes = tx.to_bytes();
            relayed_tx.set_serialized_transaction(tx_bytes);
            relayed_tx.set_max_gas(max_gas);
            relayer_db_for_events(&[relayed_tx.into()], da_height)
        }

        #[test]
        fn execute_without_commit__relayed_tx_that_passes_checks_but_fails_execution_is_reported(
        ) {
            let genesis_da_height = 3u64;
            let block_height = 1u32;
            let da_height = 10u64;
            let arb_max_gas = 10_000;

            // given
            let (tx_id, relayer_db) =
                tx_id_and_relayer_db_with_tx_that_passes_checks_but_fails_execution(
                    da_height,
                    arb_max_gas,
                );

            // when
            let on_chain_db = database_with_genesis_block(genesis_da_height);
            let producer = create_relayer_executor(on_chain_db, relayer_db);
            let block = test_block(block_height.into(), da_height.into(), 0);
            let (result, _) = producer
                .produce_without_commit(block.into())
                .unwrap()
                .into();

            // then
            let txs = result.block.transactions();
            assert_eq!(txs.len(), 2);

            // and
            let events = result.events;
            let fuel_core_types::services::executor::Event::ForcedTransactionFailed {
                failure: actual,
                ..
            } = &events[3]
            else {
                panic!("Expected `ForcedTransactionFailed` event")
            };
            let expected =
                &fuel_core_types::services::executor::Error::TransactionIdCollision(
                    tx_id,
                )
                .to_string();
            assert_eq!(expected, actual);
        }

        fn tx_id_and_relayer_db_with_tx_that_passes_checks_but_fails_execution(
            da_height: u64,
            max_gas: u64,
        ) -> (Bytes32, Database<Relayer>) {
            let mut relayed_tx = RelayedTransaction::default();
            let tx = script_tx_for_amount(100);
            let tx_bytes = tx.to_bytes();
            relayed_tx.set_serialized_transaction(tx_bytes);
            relayed_tx.set_max_gas(max_gas);
            let mut bad_relayed_tx = relayed_tx.clone();
            let new_nonce = [9; 32].into();
            bad_relayed_tx.set_nonce(new_nonce);
            let relayer_db = relayer_db_for_events(
                &[relayed_tx.into(), bad_relayed_tx.into()],
                da_height,
            );
            (tx.id(&Default::default()), relayer_db)
        }

        #[test]
        fn execute_without_commit__validation__includes_status_of_failed_relayed_tx() {
            let genesis_da_height = 3u64;
            let block_height = 1u32;
            let da_height = 10u64;
            let arb_large_max_gas = 10_000;

            // given
            let event = arb_invalid_relayed_tx_event(arb_large_max_gas);
            let produced_block = produce_block_with_relayed_event(
                event.clone(),
                genesis_da_height,
                block_height,
                da_height,
            );

            // when
            let verifyer_db = database_with_genesis_block(genesis_da_height);
            let mut verifier_relayer_db = Database::<Relayer>::default();
            let events = vec![event];
            add_events_to_relayer(&mut verifier_relayer_db, da_height.into(), &events);
            let verifier = create_relayer_executor(verifyer_db, verifier_relayer_db);
            let (result, _) = verifier.validate(&produced_block).unwrap().into();

            // then
            let txs = produced_block.transactions();
            assert_eq!(txs.len(), 1);

            // and
            let events = result.events;
            let fuel_core_types::services::executor::Event::ForcedTransactionFailed {
                failure: actual,
                ..
            } = &events[0]
            else {
                panic!("Expected `ForcedTransactionFailed` event")
            };
            let expected = &ForcedTransactionFailure::CheckError(CheckError::Validity(
                ValidityError::NoSpendableInput,
            ))
            .to_string();
            assert_eq!(expected, actual);
        }

        fn produce_block_with_relayed_event(
            event: Event,
            genesis_da_height: u64,
            block_height: u32,
            da_height: u64,
        ) -> Block {
            let producer_db = database_with_genesis_block(genesis_da_height);
            let producer_relayer_db = relayer_db_for_events(&[event], da_height);

            let producer = create_relayer_executor(producer_db, producer_relayer_db);
            let block = test_block(block_height.into(), da_height.into(), 0);
            let (produced_result, _) = producer
                .produce_without_commit(block.into())
                .unwrap()
                .into();
            produced_result.block
        }

        fn arb_invalid_relayed_tx_event(max_gas: u64) -> Event {
            let mut invalid_relayed_tx = RelayedTransaction::default();
            let mut tx = script_tx_for_amount(100);
            tx.as_script_mut().unwrap().inputs_mut().drain(..); // Remove all the inputs :)
            let tx_bytes = tx.to_bytes();
            invalid_relayed_tx.set_serialized_transaction(tx_bytes);
            invalid_relayed_tx.set_max_gas(max_gas);
            invalid_relayed_tx.into()
        }

        #[test]
        fn execute_without_commit__relayed_mint_tx_not_included_in_block() {
            let genesis_da_height = 3u64;
            let block_height = 1u32;
            let da_height = 10u64;
            let tx_count = 0;

            // given
            let relayer_db =
                relayer_db_with_mint_relayed_tx(da_height, block_height, tx_count);

            // when
            let on_chain_db = database_with_genesis_block(genesis_da_height);
            let producer = create_relayer_executor(on_chain_db, relayer_db);
            let block =
                test_block(block_height.into(), da_height.into(), tx_count as usize);
            let (result, _) = producer
                .produce_without_commit(block.into())
                .unwrap()
                .into();

            // then
            let txs = result.block.transactions();
            assert_eq!(txs.len(), 1);

            // and
            let events = result.events;
            let fuel_core_types::services::executor::Event::ForcedTransactionFailed {
                failure: actual,
                ..
            } = &events[0]
            else {
                panic!("Expected `ForcedTransactionFailed` event")
            };
            let expected = &ForcedTransactionFailure::InvalidTransactionType.to_string();
            assert_eq!(expected, actual);
        }

        fn relayer_db_with_mint_relayed_tx(
            da_height: u64,
            block_height: u32,
            tx_count: u16,
        ) -> Database<Relayer> {
            let mut relayed_tx = RelayedTransaction::default();
            let base_asset_id = AssetId::BASE;
            let mint = Transaction::mint(
                TxPointer::new(block_height.into(), tx_count),
                contract::Contract {
                    utxo_id: UtxoId::new(Bytes32::zeroed(), 0),
                    balance_root: Bytes32::zeroed(),
                    state_root: Bytes32::zeroed(),
                    tx_pointer: TxPointer::new(BlockHeight::new(0), 0),
                    contract_id: ContractId::zeroed(),
                },
                output::contract::Contract {
                    input_index: 0,
                    balance_root: Bytes32::zeroed(),
                    state_root: Bytes32::zeroed(),
                },
                0,
                base_asset_id,
                0,
            );
            let tx = Transaction::Mint(mint);
            let tx_bytes = tx.to_bytes();
            relayed_tx.set_serialized_transaction(tx_bytes);
            relayer_db_for_events(&[relayed_tx.into()], da_height)
        }

        fn relayer_db_for_events(events: &[Event], da_height: u64) -> Database<Relayer> {
            let mut relayer_db = Database::<Relayer>::default();
            add_events_to_relayer(&mut relayer_db, da_height.into(), events);
            relayer_db
        }

        #[test]
        fn execute_without_commit__relayed_tx_can_spend_message_from_same_da_block() {
            let genesis_da_height = 3u64;
            let block_height = 1u32;
            let da_height = 10u64;
            let arb_max_gas = 10_000;

            // given
            let relayer_db =
                relayer_db_with_relayed_tx_spending_message_from_same_da_block(
                    da_height,
                    arb_max_gas,
                );

            // when
            let on_chain_db = database_with_genesis_block(genesis_da_height);
            let producer =
                create_relayer_executor(on_chain_db.clone(), relayer_db.clone());
            let block = test_block(block_height.into(), da_height.into(), 0);
            let (result, _) = producer
                .produce_without_commit(block.into())
                .unwrap()
                .into();

            // then
            let txs = result.block.transactions();
            assert_eq!(txs.len(), 2);

            let validator = create_relayer_executor(on_chain_db, relayer_db);
            // When
            let result = validator.validate(&result.block).map(|_| ());

            // Then
            assert_eq!(Ok(()), result);
        }

        fn relayer_db_with_relayed_tx_spending_message_from_same_da_block(
            da_height: u64,
            max_gas: u64,
        ) -> Database<Relayer> {
            let mut relayer_db = Database::<Relayer>::default();
            let mut message = Message::default();
            let nonce = 1.into();
            message.set_da_height(da_height.into());
            message.set_nonce(nonce);
            let message_event = Event::Message(message);

            let mut relayed_tx = RelayedTransaction::default();
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
            let tx_bytes = tx.to_bytes();
            relayed_tx.set_serialized_transaction(tx_bytes);
            relayed_tx.set_max_gas(max_gas);
            let tx_event = Event::Transaction(relayed_tx);
            add_events_to_relayer(
                &mut relayer_db,
                da_height.into(),
                &[message_event, tx_event],
            );
            relayer_db
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
            let (result, changes) = producer
                .produce_without_commit(block.into())
                .unwrap()
                .into();

            // Then
            let view = ChangesIterator::<OnChain>::new(&changes);
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
            let (result, changes) = producer
                .produce_without_commit(block.into())
                .unwrap()
                .into();

            // Then
            let view = ChangesIterator::<OnChain>::new(&changes);
            assert!(result.skipped_transactions.is_empty());
            assert_eq!(view.iter_all::<Messages>(None).count() as u64, 0);
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
