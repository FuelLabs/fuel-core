use fuel_core_types::{
    fuel_asm::{
        op,
        Instruction,
        RegId,
    },
    fuel_tx::Address,
    fuel_vm::CallFrame,
};

pub fn generate(address: Address) -> Vec<u8> {
    let start_jump = vec![
        // Jump over the embedded address, which is placed immediately after the jump
        op::ji((1 + (Address::LEN / Instruction::SIZE)).try_into().unwrap()),
    ];

    let body = vec![
        // Load pointer to AssetId memory address in call frame param a
        op::addi(0x10, RegId::FP, CallFrame::a_offset().try_into().unwrap()),
        op::lw(0x10, 0x10, 0),
        // Get the balance of asset ID in the contract
        op::bal(0x11, 0x10, RegId::FP),
        // If balance == 0, return early
        op::jnzf(0x11, RegId::ZERO, 1),
        op::ret(RegId::ONE),
        // Pointer to the embedded address
        op::addi(0x12, RegId::IS, Instruction::SIZE.try_into().unwrap()),
        // Perform the transfer
        op::tro(0x12, RegId::ONE, 0x11, 0x10),
        // Return
        op::ret(RegId::ONE),
    ];

    let mut asm_bytes: Vec<u8> = start_jump.into_iter().collect();
    asm_bytes.extend_from_slice(address.as_slice()); // Embed the address
    let body: Vec<u8> = body.into_iter().collect();
    asm_bytes.extend(body.as_slice());

    asm_bytes
}

#[cfg(test)]
mod tests {
    // TODO: Tests to write for this contract:
    // 1. Happy path withdrawal case for block producer
    //    Deploy the contract for the block producer's address
    //    Run some blocks and accumulate fees
    //    Attempt to withdraw the collected fees to the BP's address
    //      (note that currently we allow anyone to initiate this withdrawal)
    // 2. Unhappy case where tx doesn't have the expected variable output set
    // 3. Edge case, withdrawal is attempted when there are no fees collected (shouldn't revert, but the BP's balance should be the same)

    use super::*;
    use crate::SecretKey;

    use rand::{
        rngs::StdRng,
        Rng,
        SeedableRng,
    };

    use fuel_core::service::{
        Config,
        FuelService,
    };
    use fuel_core_client::client::{
        types::TransactionStatus,
        FuelClient,
    };
    use fuel_core_types::{
        fuel_tx::{
            Cacheable,
            Finalizable,
            Input,
            Output,
            TransactionBuilder,
            TxParameters,
            Witness,
        },
        fuel_types::{
            AssetId,
            BlockHeight,
            ChainId,
            ContractId,
            Salt,
        },
        fuel_vm::{
            consts::VM_MAX_RAM,
            script_with_data_offset,
        },
    };

    struct TestContext {
        address: Address,
        contract_id: ContractId,
        _node: FuelService,
        client: FuelClient,
    }

    async fn setup(rng: &mut StdRng) -> TestContext {
        // Make contract that coinbase fees are collected into
        let address: Address = rng.gen();
        let salt: Salt = rng.gen();
        let contract = generate(address);
        let witness: Witness = contract.into();
        let mut create_tx = TransactionBuilder::create(witness.clone(), salt, vec![])
            .add_random_fee_input()
            .finalize();
        create_tx
            .precompute(&ChainId::default())
            .expect("tx should be valid");
        let contract_id = create_tx.metadata().as_ref().unwrap().contract_id;

        // Start up a node
        let mut config = Config::local_node();
        config.debug = true;
        config.block_producer.coinbase_recipient = Some(contract_id);
        let node = FuelService::new_node(config).await.unwrap();
        let client = FuelClient::from(node.bound_address);

        // Submit contract creation tx
        let tx_status = client
            .submit_and_await_commit(&create_tx.into())
            .await
            .unwrap();
        assert!(matches!(tx_status, TransactionStatus::Success { .. }));
        let bh = client.produce_blocks(1, None).await.unwrap();
        assert_eq!(bh, BlockHeight::new(2));

        // No fees should have been collected yet
        let contract_balance =
            client.contract_balance(&(contract_id), None).await.unwrap();
        assert_eq!(contract_balance, 0);

        TestContext {
            address,
            contract_id,
            _node: node,
            client,
        }
    }

    async fn collect_fees(ctx: &TestContext) {
        let TestContext {
            client,
            address,
            contract_id,
            ..
        } = ctx;

        // Now call the fee collection contract to withdraw the fees
        let (script, _) = script_with_data_offset!(
            data_offset,
            vec![
                op::movi(0x10, AssetId::LEN.try_into().unwrap()),
                op::aloc(0x10),
                op::movi(0x10, data_offset),
                op::call(0x10, RegId::ZERO, RegId::ZERO, RegId::CGAS),
                op::ret(RegId::ONE),
            ],
            TxParameters::DEFAULT.tx_offset()
        );
        let tx = TransactionBuilder::script(
            script.into_iter().collect(),
            (*contract_id)
                .into_iter()
                .chain((VM_MAX_RAM.checked_sub(AssetId::LEN as u64)).unwrap().to_be_bytes())
                .chain(0u64.to_be_bytes())
                .collect(),
        )
        .add_random_fee_input() // No coinbase fee for this block
        .gas_price(0)
        .script_gas_limit(1_000_000)
        .add_input(Input::contract(
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            *contract_id,
        ))
        .add_output(Output::contract(1, Default::default(), Default::default()))
        .add_output(Output::variable(
            Default::default(),
            Default::default(),
            Default::default(),
        ))
        .finalize_as_transaction();

        let tx_status = client.submit_and_await_commit(&tx).await.unwrap();
        assert!(matches!(tx_status, TransactionStatus::Success { .. }));

        // Make sure that the full balance was been withdrawn
        let contract_balance = client.contract_balance(contract_id, None).await.unwrap();
        assert_eq!(contract_balance, 0);

        // Make sure that the full balance was been withdrawn
        let asset_balance = client.balance(address, None).await.unwrap();
        assert_eq!(asset_balance, 10);
    }

    #[tokio::test]
    async fn happy_path() {
        let rng = &mut StdRng::seed_from_u64(0);

        let ctx = setup(rng).await;

        for i in 0..10 {
            // Run a script that does nothing, but will cause fee collection
            let tx = TransactionBuilder::script(
                [op::ret(RegId::ONE)].into_iter().collect(),
                vec![],
            )
            .add_unsigned_coin_input(
                SecretKey::random(rng),
                rng.gen(),
                1000,
                Default::default(),
                Default::default(),
                Default::default(),
            )
            .gas_price(1)
            .script_gas_limit(1_000_000)
            .finalize_as_transaction();
            let tx_status = ctx.client.submit_and_await_commit(&tx).await.unwrap();
            assert!(matches!(tx_status, TransactionStatus::Success { .. }));

            // Now the coinbase fee should be reflected in the contract balance
            let contract_balance = ctx
                .client
                .contract_balance(&ctx.contract_id, None)
                .await
                .unwrap();
            assert_eq!(contract_balance, i + 1);
        }

        collect_fees(&ctx).await;

        // Make sure that the full balance was been withdrawn
        let contract_balance = ctx
            .client
            .contract_balance(&ctx.contract_id, None)
            .await
            .unwrap();
        assert_eq!(contract_balance, 0);

        // Make sure that the full balance was been withdrawn
        let asset_balance = ctx.client.balance(&ctx.address, None).await.unwrap();
        assert_eq!(asset_balance, 10);
    }

    #[tokio::test]
    async fn no_fees_collected_yet() {
        let rng = &mut StdRng::seed_from_u64(0);

        // Make contract that coinbase fees are collected into
        let address: Address = rng.gen();
        let salt: Salt = rng.gen();
        let contract = generate(address);
        let witness: Witness = contract.into();
        let mut create_tx = TransactionBuilder::create(witness.clone(), salt, vec![])
            .add_random_fee_input()
            .finalize();
        create_tx
            .precompute(&ChainId::default())
            .expect("tx should be valid");
        let contract_id = create_tx.metadata().as_ref().unwrap().contract_id;

        // Start up a node
        let mut config = Config::local_node();
        config.debug = true;
        config.block_producer.coinbase_recipient = Some(contract_id);
        let node = FuelService::new_node(config).await.unwrap();
        let client = FuelClient::from(node.bound_address);

        // Submit contract creation tx
        let tx_status = client
            .submit_and_await_commit(&create_tx.into())
            .await
            .unwrap();
        assert!(matches!(tx_status, TransactionStatus::Success { .. }));
        let bh = client.produce_blocks(1, None).await.unwrap();
        assert_eq!(bh, BlockHeight::new(2));

        // No fees should have been collected yet
        let contract_balance =
            client.contract_balance(&(contract_id), None).await.unwrap();
        assert_eq!(contract_balance, 0);

        // Now call the fee collection contract to withdraw the fees
        let (script, _) = script_with_data_offset!(
            data_offset,
            vec![
                op::movi(0x10, AssetId::LEN.try_into().unwrap()),
                op::aloc(0x10),
                op::movi(0x10, data_offset),
                op::call(0x10, RegId::ZERO, RegId::ZERO, RegId::CGAS),
                op::ret(RegId::ONE),
            ],
            TxParameters::DEFAULT.tx_offset()
        );
        let tx = TransactionBuilder::script(
            script.into_iter().collect(),
            (*contract_id)
                .into_iter()
                .chain((VM_MAX_RAM - (AssetId::LEN as u64)).to_be_bytes())
                .chain(0u64.to_be_bytes())
                .collect(),
        )
        .add_random_fee_input() // No coinbase fee for this block
        .gas_price(0)
        .script_gas_limit(1_000_000)
        .add_input(Input::contract(
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            contract_id,
        ))
        .add_output(Output::contract(1, Default::default(), Default::default()))
        .add_output(Output::variable(
            Default::default(),
            Default::default(),
            Default::default(),
        ))
        .finalize_as_transaction();

        let tx_status = client.submit_and_await_commit(&tx).await.unwrap();
        assert!(matches!(tx_status, TransactionStatus::Success { .. }));

        // Make sure that the full balance was been withdrawn
        let contract_balance =
            client.contract_balance(&(contract_id), None).await.unwrap();
        assert_eq!(contract_balance, 0);

        // Make sure that the full balance was been withdrawn
        let asset_balance = client.balance(&address, None).await.unwrap();
        assert_eq!(asset_balance, 0);
    }
}
