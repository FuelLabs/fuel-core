mod collision_detector;
mod dependent_transactions;
mod executable_transactions;
mod resolution_queue;

use crate::{
    containers::{
        max_fee_per_gas_sort::{
            RatioMaxGasPriceSort,
            RatioMaxGasPriceSortKey,
        },
        sort::SortableKey,
    },
    ports::TxPoolDb,
    service::TxStatusChange,
    txpool_v2::{
        dependent_transactions::DependentTransactions,
        executable_transactions::{
            ExecutableTransactions,
            InsertionResult as ExecutableInsertionResult,
            RemoveReason,
            RemovedTransaction,
            RemovedTransactions,
        },
        resolution_queue::{
            ResolutionInput,
            ResolutionQueue,
        },
    },
    Config,
    TxInfo,
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
    tai64::Tai64,
};
use num_rational::Ratio;
use std::sync::Arc;

/// Dependent transaction is not ready to be executed until all of its parents are executed.
pub struct DependentTransaction {
    tx: ArcPoolTx,
    parents: Vec<ArcPoolTx>,
}

pub struct TxPool<D> {
    max_pool_gas: u64,
    current_pool_gas: u64,
    database: D,
    max_gas_price_sort: RatioMaxGasPriceSort,
    tx_status_change: TxStatusChange,
    config: Config,
    executable_transactions: ExecutableTransactions,
    dependent_transactions: DependentTransactions,
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
    pub fn insert(&mut self, tx: Checked<Transaction>) -> Result<(), Error> {
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

        self.insert_inner(tx, &view)
    }

    fn insert_inner(&mut self, tx: ArcPoolTx, view: &View) -> Result<(), Error> {
        let info = TxInfo::new(tx);

        // The function will remove transactions with the lowest gas price,
        // even if, at the end, a new transaction will be rejected.
        // It opens space for the next upcoming transaction.
        // It will happen only when the TxPool gas limit is reached.
        // It is acceptable since we already have a lot of transactions to execute.
        self.crowd_out_transactions(&info)?;

        let stage = transaction_stage(
            info.clone(),
            &view,
            &self.executable_transactions,
            &self.dependent_transactions,
        )?;

        let resolved_transactions = match stage {
            TransactionStage::Executable(tx) => Some(self.insert_executable(tx)?),
            TransactionStage::Dependent(tx, parents) => {
                Some(self.insert_dependent(tx, parents)?)
            }
            TransactionStage::Unresolved(tx, inputs) => {
                self.insert_unresolved(tx, inputs);
                None
            }
        };

        self.current_pool_gas = self.current_pool_gas.saturating_add(info.tx.max_gas());
        self.max_gas_price_sort.insert(&info);
        self.tx_status_change.send_submitted(
            tx.id(),
            Tai64::from_unix(info.submitted_time.as_secs() as i64),
        );

        if let Some(resolved_transactions) = resolved_transactions {
            for resolved_transaction in resolved_transactions {
                let tx_id = resolved_transaction.id();
                let result = self.insert_inner(resolved_transaction, view);

                if let Err(error) = result {
                    self.tx_status_change.send_squeezed_out(tx_id, error);
                }
            }
        }

        Ok(())
    }

    fn contains(&self, tx_id: &TxId, view: &View) -> Result<bool, Error> {
        Ok(view.contains_tx(tx_id)?
            || self.executable_transactions.contains(tx_id)
            || self.dependent_transactions.contains(tx_id)
            || self.resolution_queue.contains(tx_id))
    }

    fn insert_executable(&mut self, info: TxInfo) -> Result<Vec<ArcPoolTx>, Error> {
        let result = self.executable_transactions.insert(info)?;

        // Below this point, the transaction must be inserted -> no errors.

        let (added_coins, removed_transactions) = match result {
            ExecutableInsertionResult::Successfully(coins) => (coins, None),
            ExecutableInsertionResult::SuccessfullyWithRemovedTransactions {
                upcoming_coins,
                removed_transaction,
            } => (upcoming_coins, Some(removed_transaction)),
        };

        if let Some(removed_transactions) = removed_transactions {
            for removed_transaction in removed_transactions {
                self.process_removed_transaction(removed_transaction);
            }
        }

        let resolved_transactions = self.resolution_queue.resolve(added_coins);

        Ok(resolved_transactions)
    }

    fn insert_dependent(&mut self, info: TxInfo) -> Result<Vec<ArcPoolTx>, Error> {
        let result = self.dependent_transactions.insert(info)?;

        // Below this point, the transaction must be inserted -> no errors.

        let (added_coins, removed_transactions) = match result {
            ExecutableInsertionResult::Successfully(coins) => (coins, None),
            ExecutableInsertionResult::SuccessfullyWithRemovedTransactions {
                upcoming_coins,
                removed_transaction,
            } => (upcoming_coins, Some(removed_transaction)),
        };

        if let Some(removed_transactions) = removed_transactions {
            for removed_transaction in removed_transactions {
                self.process_removed_transaction(removed_transaction);
            }
        }

        let resolved_transactions = self.resolution_queue.resolve(added_coins);

        Ok(resolved_transactions)
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

    fn crowd_out_transactions(&mut self, info: &TxInfo) -> Result<(), Error> {
        let new_tx_id = info.tx().id();
        let new_tx_key = RatioMaxGasPriceSortKey::new(info);
        let target_gas = self.max_pool_gas.saturating_sub(info.tx.max_gas());

        while self.current_pool_gas > target_gas {
            let Some((key, tx_to_remove)) = self.max_gas_price_sort.lowest() else {
                unreachable!(
                    "The `current_pool_gas` is non zero, \
                    so there should be at least one transaction in the pool"
                );
            };

            if key >= &new_tx_key {
                return Err(Error::NotInsertedLimitHit);
            }

            let tx_id = tx_to_remove.id();

            self.remove(&tx_id, RemoveReason::UpcomingTxCrowdOutLowestTx(new_tx_id));
        }

        Ok(())
    }

    fn remove(&mut self, tx_id: &TxId, reason: RemoveReason) {
        if let Some(removed_transaction) =
            self.executable_transactions.remove(tx_id, reason.clone())
        {
            self.process_removed_transaction(removed_transaction)
        }
        if let Some(removed_transaction) =
            self.dependent_transactions.remove(tx_id, reason.clone())
        {
            self.process_removed_transaction(removed_transaction)
        }
        if let Some(removed_transaction) = self.resolution_queue.remove(tx_id, reason) {
            self.process_removed_transaction(removed_transaction)
        }
    }

    fn process_removed_transaction(&mut self, removed_transaction: RemovedTransaction) {
        let RemovedTransaction {
            info,
            removed_coins,
            reason,
        } = removed_transaction;
        self.current_pool_gas = self.current_pool_gas.saturating_sub(info.tx.max_gas());
        self.max_gas_price_sort.remove(&info);
        self.tx_status_change
            .send_squeezed_out(info.tx.id(), Error::SqueezedOut(reason.to_string()));

        let removed_dependent_transactions =
            self.dependent_transactions.remove_dependent(
                removed_coins.iter(),
                RemoveReason::ParentTransactionIsRemoved {
                    parent_tx_id: info.tx.id(),
                    remove_reason: Box::new(reason.clone()),
                },
            );

        self.resolution_queue.unresolve(removed_coins.iter());

        for removed_transaction in removed_dependent_transactions {
            self.process_removed_transaction(removed_transaction);
        }
    }
}

enum TransactionStage {
    /// Transaction is ready to be executed.
    Executable(TxInfo),
    /// Transaction is not ready to be executed until all of its parents are executed.
    /// It can be promoted to the `Executable` when all parents are included.
    Dependent(TxInfo, Vec<UtxoId>),
    /// The transaction uses not existing state. It can be promoted
    /// to the `Executable` or `Dependent` stage when new inputs arrive.
    Unresolved(TxInfo, Vec<ResolutionInput>),
}

fn transaction_stage<D>(
    info: TxInfo,
    view: &impl TxPoolDb,
    executable_transactions: &ExecutableTransactions,
    dependent_transaction: &DependentTransactions,
) -> Result<TransactionStage, Error> {
    let mut dependent_coins = vec![];
    let mut unresolved_inputs = vec![];
    for input in info.tx.inputs() {
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
        TransactionStage::Unresolved(info, unresolved_inputs)
    } else if !dependent_coins.is_empty() {
        TransactionStage::Dependent(info, dependent_coins)
    } else {
        TransactionStage::Executable(info)
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
