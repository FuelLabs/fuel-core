mod collision_detector;
mod executable_transactions;
mod resolution_queue;

use crate::{
    ports::TxPoolDb,
    txpool_v2::{
        executable_transactions::{
            ExecutableTransactions,
            InsertionResult as ExecutableInsertionResult,
            RemovedTransaction,
            RemovedTransactions,
        },
        resolution_queue::{
            ResolutionInput,
            ResolutionQueue,
        },
    },
    Config,
};
use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::{
    fuel_tx::{
        input::{
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
        Input,
        Transaction,
        TxId,
        UtxoId,
    },
    fuel_types::Nonce,
    fuel_vm::checked_transaction::{
        Checked,
        CheckedTransaction,
    },
    services::txpool::{
        ArcPoolTx,
        Error,
        PoolTransaction,
    },
};
use std::sync::Arc;

/// Dependent transaction is not ready to be executed until all of its parents are executed.
pub struct DependentTransaction {
    tx: ArcPoolTx,
    parents: Vec<ArcPoolTx>,
}

pub struct TxPool<D> {
    database: D,
    config: Config,
    executable_transactions: ExecutableTransactions,
    dependent_transactions: ExecutableTransactions,
    resolution_queue: ResolutionQueue,
}

pub struct InsertionResult {
    tx_id: TxId,
    removed_transaction: Option<RemovedTransactions>,
    resolved_new_transactions: Vec<ArcPoolTx>,
}

impl<D, View> TxPool<D>
where
    D: AtomicView<View = View>,
    View: TxPoolDb,
{
    pub fn insert(&mut self, tx: Checked<Transaction>) -> Result<InsertionResult, Error> {
        let view = self.database.latest_view();

        let tx: CheckedTransaction = tx.into();

        let tx = Arc::new(match tx {
            CheckedTransaction::Script(tx) => PoolTransaction::Script(tx),
            CheckedTransaction::Create(tx) => PoolTransaction::Create(tx),
            CheckedTransaction::Mint(_) => return Err(Error::MintIsDisallowed),
            CheckedTransaction::Upgrade(tx) => PoolTransaction::Upgrade(tx),
            CheckedTransaction::Upload(tx) => PoolTransaction::Upload(tx),
        });

        self.check_blacklisting(tx.as_ref())?;

        if !tx.is_computed() {
            return Err(Error::NoMetadata)
        }

        if self.contains(&tx.id(), &view)? {
            return Err(Error::NotInsertedTxKnown)
        }

        let stage = transaction_stage(
            tx,
            &view,
            &self.executable_transactions,
            &self.dependent_transactions,
        )?;

        match stage {
            TransactionStage::Executable(tx) => self.insert_executable(tx)?,
            TransactionStage::Dependent(tx, parents) => {
                self.insert_dependent(tx, parents)
            }
            TransactionStage::Unresolved(tx, inputs) => {
                self.insert_unresolved(tx, inputs)
            }
        };

        Ok(InsertionResult {
            tx_id: tx.id(),
            removed_transaction: vec![],
            resolved_new_transactions: vec![],
        })
    }

    fn contains(&self, tx_id: &TxId, view: &View) -> Result<bool, Error> {
        Ok(view.contains_tx(tx_id)?
            || self.executable_transactions.contains(tx_id)
            || self.dependent_transactions.contains(tx_id)
            || self.resolution_queue.contains(tx_id))
    }

    fn insert_executable(&mut self, tx: ArcPoolTx) -> Result<InsertionResult, Error> {
        let tx_id = tx.id();
        let result = self.executable_transactions.insert(tx)?;

        // Below this point, the transaction must be inserted -> no errors.

        let (added_coins, removed_transaction) = match result {
            ExecutableInsertionResult::Successfully(coins) => (coins, None),
            ExecutableInsertionResult::SuccessfullyWithRemovedTransactions {
                upcoming_coins,
                removed_transaction,
            } => (upcoming_coins, Some(removed_transaction)),
        };

        let removed_transaction =
            if let Some(mut removed_transaction) = removed_transaction {
                let removed_coins = removed_transaction
                    .iter()
                    .map(|tx| tx.removed_coins.iter())
                    .flatten()
                    .map(|utxo_id| *utxo_id)
                    .collect::<Vec<_>>();

                let removed_dependent_transactions = self
                    .dependent_transactions
                    .process_removed_coins(removed_coins);
                removed_transaction.extend(removed_dependent_transactions);

                Some(removed_transaction)
            } else {
                None
            };

        let resolved_transactions = self.resolution_queue.resolve(added_coins);

        Ok(InsertionResult {
            tx_id,
            removed_transaction,
            resolved_new_transactions: resolved_transactions,
        })
    }

    fn check_blacklisting(&self, tx: &PoolTransaction) -> Result<(), Error> {
        for input in tx.inputs() {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, owner, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, owner, .. }) => {
                    if self.config.blacklist.contains_coin(utxo_id) {
                        return Err(Error::BlacklistedUTXO(*utxo_id))
                    }
                    if self.config.blacklist.contains_address(owner) {
                        return Err(Error::BlacklistedOwner(*owner))
                    }
                }
                Input::Contract(contract) => {
                    if self
                        .config
                        .blacklist
                        .contains_contract(&contract.contract_id)
                    {
                        return Err(Error::BlacklistedContract(contract.contract_id))
                    }
                }
                Input::MessageCoinSigned(MessageCoinSigned {
                    nonce,
                    sender,
                    recipient,
                    ..
                })
                | Input::MessageCoinPredicate(MessageCoinPredicate {
                    nonce,
                    sender,
                    recipient,
                    ..
                })
                | Input::MessageDataSigned(MessageDataSigned {
                    nonce,
                    sender,
                    recipient,
                    ..
                })
                | Input::MessageDataPredicate(MessageDataPredicate {
                    nonce,
                    sender,
                    recipient,
                    ..
                }) => {
                    if self.config.blacklist.contains_message(nonce) {
                        return Err(Error::BlacklistedMessage(*nonce))
                    }
                    if self.config.blacklist.contains_address(sender) {
                        return Err(Error::BlacklistedOwner(*sender))
                    }
                    if self.config.blacklist.contains_address(recipient) {
                        return Err(Error::BlacklistedOwner(*recipient))
                    }
                }
            }
        }

        Ok(())
    }
}

enum TransactionStage {
    /// Transaction is ready to be executed.
    Executable(ArcPoolTx),
    /// Transaction is not ready to be executed until all of its parents are executed.
    /// It can be promoted to the `Executable` when all parents are included.
    Dependent(ArcPoolTx, Vec<UtxoId>),
    /// The transaction uses not existing state. It can be promoted
    /// to the `Executable` or `Dependent` stage when new inputs arrive.
    Unresolved(ArcPoolTx, Vec<ResolutionInput>),
}

fn transaction_stage<D>(
    tx: ArcPoolTx,
    view: &impl TxPoolDb,
    executable_transactions: &ExecutableTransactions,
    dependent_transaction: &ExecutableTransactions,
) -> Result<TransactionStage, Error> {
    let mut dependent_coins = vec![];
    let mut unresolved_inputs = vec![];
    for input in tx.inputs() {
        match input {
            Input::CoinSigned(CoinSigned { utxo_id, .. })
            | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                let coin = if let Some(coin) = &view.utxo(utxo_id)? {
                    coin
                } else if let Some(coin) = executable_transactions.get_coin(utxo_id) {
                    dependent_coins.push(*utxo_id);
                    coin
                } else if let Some(coin) = dependent_transaction.get_coin(utxo_id) {
                    dependent_coins.push(*utxo_id);
                    coin
                } else {
                    unresolved_inputs.push(ResolutionInput::Coin(*utxo_id));
                    continue
                };

                let comparison_result = coin
                    .matches_input(input)
                    .expect("Impossible since `input` is a coin");

                if !comparison_result {
                    return Err(Error::NotInsertedIoCoinMismatch);
                }
            }
            Input::Contract(contract) => {
                if !view.contract_exist(&contract.contract_id)? {
                    unresolved_inputs
                        .push(ResolutionInput::Contract(contract.contract_id));
                }
            }
            Input::MessageCoinSigned(MessageCoinSigned { nonce, .. })
            | Input::MessageCoinPredicate(MessageCoinPredicate { nonce, .. })
            | Input::MessageDataSigned(MessageDataSigned { nonce, .. })
            | Input::MessageDataPredicate(MessageDataPredicate { nonce, .. }) => {
                let Some(message) = view.message(nonce)? else {
                    unresolved_inputs.push(ResolutionInput::Message(*nonce));
                    continue
                };

                let comparison_result = message
                    .matches_input(input)
                    .expect("Impossible since `input` is a message");

                if !comparison_result {
                    return Err(Error::NotInsertedIoMessageMismatch);
                }
            }
        }
    }

    let stage = if !unresolved_inputs.is_empty() {
        TransactionStage::Unresolved(tx, unresolved_inputs)
    } else if !dependent_coins.is_empty() {
        TransactionStage::Dependent(tx, dependent_coins)
    } else {
        TransactionStage::Executable(tx)
    };

    Ok(stage)
}

pub trait CoinMessage {
    fn coin(&self) -> Option<&UtxoId>;
    fn message(&self) -> Option<&Nonce>;
}

impl CoinMessage for Input {
    fn coin(&self) -> Option<&UtxoId> {
        match self {
            Input::CoinPredicate(coin) => Some(&coin.utxo_id),
            Input::CoinSigned(coin) => Some(&coin.utxo_id),
            _ => None,
        }
    }

    fn message(&self) -> Option<&Nonce> {
        match self {
            Input::MessageCoinSigned(message) => Some(&message.nonce),
            Input::MessageCoinPredicate(message) => Some(&message.nonce),
            Input::MessageDataSigned(message) => Some(&message.nonce),
            Input::MessageDataPredicate(message) => Some(&message.nonce),
            _ => None,
        }
    }
}
