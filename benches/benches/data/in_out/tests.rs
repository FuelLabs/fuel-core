use fuel_core::{
    executor::Executor,
    service::{
        adapters::MaybeRelayerAdapter,
        Config,
    },
};
use fuel_core_benches::Database;
use fuel_core_storage::{
    tables::{
        Coins,
        ContractsLatestUtxo,
        Messages,
    },
    StorageAsMut,
};
use fuel_core_types::{
    blockchain::block::PartialFuelBlock,
    entities::{
        coins::coin::CompressedCoin,
        contract::ContractUtxoInfo,
    },
    fuel_asm::op,
    fuel_tx::{
        field::Inputs,
        Cacheable,
        ConsensusParameters,
        Executable,
        Signable,
        Transaction,
    },
    fuel_vm::GasCosts,
    services::executor::ExecutionBlock,
};

use crate::data::make_header;

use super::*;

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
        config,
    };
    let mut test_data = InputOutputData::default();
    let mut data = Data::default();

    <out_ty::Void as ValidTx<in_ty::CoinSigned>>::fill(&mut data, &mut test_data, 5);
    <(out_ty::Coin, out_ty::Contract) as ValidTx<(
        in_ty::MessageData,
        in_ty::Contract,
    )>>::fill(&mut data, &mut test_data, 5);

    let t = into_txn(test_data);
    insert_into_db(&mut executor.database, &t);
    test_transaction(&executor, t);
}

fn insert_into_db(db: &mut Database, transaction: &Transaction) {
    match transaction {
        Transaction::Script(s) => {
            for input in s.inputs() {
                match input {
                    Input::CoinSigned(c) => {
                        let coin = CompressedCoin {
                            owner: c.owner,
                            amount: c.amount,
                            asset_id: c.asset_id,
                            maturity: c.maturity,
                            tx_pointer: c.tx_pointer,
                        };

                        db.storage::<Coins>().insert(&c.utxo_id, &coin).unwrap();
                    }
                    Input::CoinPredicate(c) => {
                        let coin = CompressedCoin {
                            owner: c.owner,
                            amount: c.amount,
                            asset_id: c.asset_id,
                            maturity: c.maturity,
                            tx_pointer: c.tx_pointer,
                        };

                        db.storage::<Coins>().insert(&c.utxo_id, &coin).unwrap();
                    }
                    Input::MessageCoinSigned(m) => {
                        let m = fuel_core_types::entities::message::Message {
                            sender: m.sender,
                            recipient: m.recipient,
                            nonce: m.nonce,
                            amount: m.amount,
                            data: Vec::with_capacity(0),
                            da_height: 0u64.into(),
                        };
                        db.storage::<Messages>().insert(&m.nonce, &m).unwrap();
                    }
                    Input::MessageDataPredicate(m) => {
                        let m = fuel_core_types::entities::message::Message {
                            sender: m.sender,
                            recipient: m.recipient,
                            nonce: m.nonce,
                            amount: m.amount,
                            data: m.data.clone(), 
                            da_height: 0u64.into(),
                        };
                        db.storage::<Messages>().insert(&m.nonce, &m).unwrap();
                    }
                    Input::Contract(c) => {
                        db.storage::<ContractsLatestUtxo>()
                            .insert(
                                &c.contract_id,
                                &ContractUtxoInfo {
                                    utxo_id: c.utxo_id,
                                    tx_pointer: c.tx_pointer,
                                },
                            )
                            .unwrap();
                    }
                    _ => (),
                }
            }
        }
        Transaction::Create(_) => (),
        _ => (),
    }
}

fn into_txn(data: InputOutputData) -> Transaction {
    let InputOutputData {
        inputs,
        outputs,
        witnesses,
        secrets,
    } = data;
    let mut script = Transaction::script(
        0,
        0,
        0.into(),
        [op::ret(0)].into_iter().collect(),
        vec![],
        inputs,
        outputs,
        witnesses,
    );
    script.prepare_init_script();

    for secret in secrets {
        script.sign_inputs(&secret, &ConsensusParameters::default());
    }

    script.precompute(&ConsensusParameters::default());
    script.into()
}

fn test_transaction(executor: &Executor<MaybeRelayerAdapter>, transaction: Transaction) {
    let header = make_header();
    let block = PartialFuelBlock::new(header, vec![transaction]);
    let block = ExecutionBlock::Production(block);

    let result = executor.execute_without_commit(block).unwrap();
    let status = result.result().tx_status.clone();
    let errors = result
        .result()
        .skipped_transactions
        .iter()
        .map(|(_, e)| e)
        .collect::<Vec<_>>();
    //         eprintln!("Transaction failed: {params}, {errors:?}, {status:?}");

    assert!(
        result.result().skipped_transactions.is_empty(),
        "Skipped transactions: {errors:?}, {status:?}"
    );
}
