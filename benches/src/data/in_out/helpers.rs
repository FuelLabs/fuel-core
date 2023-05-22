use fuel_core_storage::{
    tables::{
        Coins,
        ContractsInfo,
        ContractsLatestUtxo,
        ContractsRawCode,
        Messages,
    },
    StorageAsMut,
};
use fuel_core_types::{
    entities::{
        coins::coin::CompressedCoin,
        contract::ContractUtxoInfo,
    },
    fuel_tx::{
        field::Inputs,
        Cacheable,
        ConsensusParameters,
        Executable,
        Signable,
        Transaction,
    },
};

use super::*;

pub fn insert_into_db(db: &mut Database, transaction: &Transaction, data: &mut Data) {
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
                        let contract = data.contract();
                        db.storage::<ContractsLatestUtxo>()
                            .insert(
                                &c.contract_id,
                                &ContractUtxoInfo {
                                    utxo_id: c.utxo_id,
                                    tx_pointer: c.tx_pointer,
                                },
                            )
                            .unwrap();
                        db.storage::<ContractsInfo>()
                            .insert(&c.contract_id, &(data.salt(), contract.root()))
                            .unwrap();
                        db.storage::<ContractsRawCode>()
                            .insert(&c.contract_id, contract.as_ref())
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

pub fn into_script_txn(data: InputOutputData) -> Transaction {
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
