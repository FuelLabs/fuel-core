use std::sync::Arc;

use crate::Database;
use fuel_core::{
    executor::{
        ExecutionOptions,
        Executor,
    },
    service::{
        adapters::MaybeRelayerAdapter,
        Config,
    },
};
use fuel_core_types::{
    blockchain::block::PartialFuelBlock,
    fuel_tx::Transaction,
    fuel_vm::GasCosts,
    services::executor::ExecutionBlock,
};

use crate::data::make_header;

use super::{
    helpers::{
        insert_into_db,
        into_script_txn,
    },
    *,
};

/// Test the benches are valid.
/// This is way faster then running the benches.
#[test]
fn test_in_out() {
    let database = Database::default();
    let relayer = MaybeRelayerAdapter {
        database: database.clone(),
        relayer_synced: None,
        da_deploy_height: 0u64.into(),
    };
    let mut config = Config::local_node();
    config.chain_conf.gas_costs = GasCosts::free();
    config.utxo_validation = true;
    let mut executor = Executor {
        database,
        relayer,
        config: Arc::new(config.block_executor),
    };
    let mut test_data = InputOutputData::default();
    let mut data = Data::default();

    <out_ty::Void as ValidTx<in_ty::CoinSigned>>::fill(&mut data, &mut test_data, 5);
    <out_ty::Coin as ValidTx<in_ty::CoinSigned>>::fill(&mut data, &mut test_data, 5);
    <out_ty::Variable as ValidTx<in_ty::CoinSigned>>::fill(&mut data, &mut test_data, 5);
    <out_ty::Change as ValidTx<in_ty::CoinSigned>>::fill(&mut data, &mut test_data, 5);
    <(out_ty::Coin, out_ty::Contract) as ValidTx<(
        in_ty::MessageData,
        in_ty::Contract,
    )>>::fill(&mut data, &mut test_data, 5);

    let t = into_script_txn(test_data);
    insert_into_db(&mut executor.database, &t, &mut data);
    test_transaction(&executor, t);
}

fn test_transaction(executor: &Executor<MaybeRelayerAdapter>, transaction: Transaction) {
    let header = make_header();
    let block = PartialFuelBlock::new(header, vec![transaction]);
    let block = ExecutionBlock::Production(block);

    let result = executor
        .execute_without_commit(
            block,
            ExecutionOptions {
                utxo_validation: true,
            },
        )
        .unwrap();
    let status = result.result().tx_status.clone();
    let errors = result
        .result()
        .skipped_transactions
        .iter()
        .map(|(_, e)| e)
        .collect::<Vec<_>>();

    assert!(
        result.result().skipped_transactions.is_empty(),
        "Skipped transactions: {errors:?}, {status:?}"
    );
}
