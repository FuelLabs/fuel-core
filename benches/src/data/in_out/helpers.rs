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

/// Inserts the transaction data into the database.
pub fn insert_into_db(db: &mut Database, transaction: &Transaction, data: &mut Data) {
    match transaction {
        Transaction::Script(s) => {
            // Iterate over each input in the script
            for input in s.inputs() {
                match input {
                    Input::CoinSigned(c) => {
                        // Create a compressed coin and insert it into the Coins database storage
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
                        // Create a compressed coin and insert it into the Coins database storage
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
                        // Create a message and insert it into the Messages database storage
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
                        // Create a message and insert it into the Messages database storage
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
                        // Insert contract information into the ContractsLatestUtxo, ContractsInfo,
                        // and ContractsRawCode database storage
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

/// Converts the input-output data into a script transaction.
pub fn into_script_txn(data: InputOutputData) -> Transaction {
    let InputOutputData {
        inputs,
        outputs,
        witnesses,
        secrets,
    } = data;
    // Create an empty script transaction
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

    // Prepare the script
    script.prepare_init_script();

    // Sign inputs with secrets
    for secret in secrets {
        script.sign_inputs(&secret, &ConsensusParameters::default());
    }

    // Precompute the script
    script.precompute(&ConsensusParameters::default());

    // Return the script as a transaction
    script.into()
}
