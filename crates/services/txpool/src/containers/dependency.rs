use crate::{
    ports::TxPoolDb,
    types::*,
    Error,
    TxInfo,
};
use fuel_core_types::{
    fuel_tx::{
        input::{
            coin::{
                CoinPredicate,
                CoinSigned,
            },
            contract::Contract,
            message::{
                MessageCoinPredicate,
                MessageCoinSigned,
                MessageDataPredicate,
                MessageDataSigned,
            },
        },
        Input,
        Output,
        UtxoId,
    },
    fuel_types::Nonce,
    services::txpool::ArcPoolTx,
};
use std::collections::{
    HashMap,
    HashSet,
};
use tracing::warn;

/// Check and hold dependency between inputs and outputs. Be mindful
/// about depth of connection
#[derive(Debug, Clone)]
pub struct Dependency {
    /// mapping of all UtxoId relationships in txpool
    coins: HashMap<UtxoId, CoinState>,
    /// Contract-> Tx mapping.
    contracts: HashMap<ContractId, ContractState>,
    /// messageId -> tx mapping
    messages: HashMap<Nonce, MessageState>,
    /// max depth of dependency.
    max_depth: usize,
    /// utxo-validation feature flag
    utxo_validation: bool,
}

#[derive(Debug, Clone)]
pub struct CoinState {
    /// is Utxo spend as other Tx input
    is_spend_by: Option<TxId>,
    /// how deep are we inside UTXO dependency
    depth: usize,
}

impl CoinState {
    /// Indicates whether coin exists in the database
    pub fn is_in_database(&self) -> bool {
        self.depth == 0
    }
}

#[derive(Debug, Clone)]
pub struct ContractState {
    /// is Utxo spend as other Tx input
    used_by: HashSet<TxId>,
    /// how deep are we inside UTXO dependency
    depth: usize,
    /// origin is needed for child to parent rel, in case when contract is in dependency this is how we make a chain.
    origin: Option<UtxoId>,
    /// gas_price. We can probably derive this from Tx
    gas_price: GasPrice,
}

impl ContractState {
    /// Indicates whether the contract exists in the database
    pub fn is_in_database(&self) -> bool {
        self.depth == 0
    }
}

/// Always in database. No need for optional spenders, as this state would just be removed from
/// the hashmap if the message id isn't being spent.
#[derive(Debug, Clone)]
pub struct MessageState {
    spent_by: TxId,
    gas_price: GasPrice,
}

impl Dependency {
    pub fn new(max_depth: usize, utxo_validation: bool) -> Self {
        Self {
            coins: HashMap::new(),
            contracts: HashMap::new(),
            messages: HashMap::new(),
            max_depth,
            utxo_validation,
        }
    }

    /// find all dependent Transactions that are inside txpool.
    /// Does not check db. They can be sorted by gasPrice to get order of dependency
    pub(crate) fn find_dependent(
        &self,
        tx: ArcPoolTx,
        seen: &mut HashMap<TxId, ArcPoolTx>,
        txs: &HashMap<TxId, TxInfo>,
    ) {
        // for every input aggregate UtxoId and check if it is inside
        let mut check = vec![tx.id()];
        while let Some(parent_txhash) = check.pop() {
            let mut is_new = false;
            let mut parent_tx = None;
            seen.entry(parent_txhash).or_insert_with(|| {
                is_new = true;
                let parent = txs.get(&parent_txhash).expect("To have tx in txpool");
                parent_tx = Some(parent.clone());
                parent.tx().clone()
            });
            // for every input check if tx_id is inside seen. if not, check coins/contract map.
            if let Some(parent_tx) = parent_tx {
                for input in parent_tx.inputs() {
                    // if found and depth is not zero add it to `check`.
                    match input {
                        Input::CoinSigned(CoinSigned { utxo_id, .. })
                        | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                            let state = self
                                .coins
                                .get(utxo_id)
                                .expect("to find coin inside spend tx");
                            if !state.is_in_database() {
                                check.push(*utxo_id.tx_id());
                            }
                        }
                        Input::Contract(Contract { contract_id, .. }) => {
                            let state = self
                                .contracts
                                .get(contract_id)
                                .expect("Expect to find contract in dependency");

                            if !state.is_in_database() {
                                let origin = state
                                    .origin
                                    .as_ref()
                                    .expect("contract origin to be present");
                                check.push(*origin.tx_id());
                            }
                        }
                        Input::MessageCoinSigned(_)
                        | Input::MessageCoinPredicate(_)
                        | Input::MessageDataSigned(_)
                        | Input::MessageDataPredicate(_) => {
                            // Message inputs do not depend on any other fuel transactions
                        }
                    }
                }
            }
        }
    }

    fn check_if_coin_input_can_spend_output(
        output: &Output,
        input: &Input,
        is_output_filled: bool,
    ) -> Result<(), Error> {
        if let Input::CoinSigned(CoinSigned {
            owner,
            amount,
            asset_id,
            ..
        })
        | Input::CoinPredicate(CoinPredicate {
            owner,
            amount,
            asset_id,
            ..
        }) = input
        {
            let i_owner = owner;
            let i_amount = amount;
            let i_asset_id = asset_id;
            match output {
                Output::Coin {
                    to,
                    amount,
                    asset_id,
                } => {
                    if to != i_owner {
                        return Err(Error::NotInsertedIoWrongOwner)
                    }
                    if amount != i_amount {
                        return Err(Error::NotInsertedIoWrongAmount)
                    }
                    if asset_id != i_asset_id {
                        return Err(Error::NotInsertedIoWrongAssetId)
                    }
                }
                Output::Contract(_) => return Err(Error::NotInsertedIoContractOutput),
                Output::Change {
                    to,
                    asset_id,
                    amount,
                } => {
                    if to != i_owner {
                        return Err(Error::NotInsertedIoWrongOwner)
                    }
                    if asset_id != i_asset_id {
                        return Err(Error::NotInsertedIoWrongAssetId)
                    }
                    if is_output_filled && amount != i_amount {
                        return Err(Error::NotInsertedIoWrongAmount)
                    }
                }
                Output::Variable {
                    to,
                    amount,
                    asset_id,
                } => {
                    if is_output_filled {
                        if to != i_owner {
                            return Err(Error::NotInsertedIoWrongOwner)
                        }
                        if amount != i_amount {
                            return Err(Error::NotInsertedIoWrongAmount)
                        }
                        if asset_id != i_asset_id {
                            return Err(Error::NotInsertedIoWrongAssetId)
                        }
                    }
                    // else do nothing, everything is variable and can be only check on execution
                }
                Output::ContractCreated { .. } => {
                    return Err(Error::NotInsertedIoContractOutput)
                }
            };
        } else {
            return Err(Error::Other(
                "Use it only for coin output check".to_string(),
            ))
        }
        Ok(())
    }

    /// Check for collision. Used only inside insert function.
    /// Id doesn't change any dependency it just checks if it has possibility to be included.
    /// Returns: (max_depth, db_coins, db_contracts, collided_transactions);
    #[allow(clippy::type_complexity)]
    fn check_for_collision<'a>(
        &'a self,
        txs: &'a HashMap<TxId, TxInfo>,
        db: &dyn TxPoolDb,
        tx: &'a ArcPoolTx,
    ) -> Result<
        (
            usize,
            HashMap<UtxoId, CoinState>,
            HashMap<ContractId, ContractState>,
            HashMap<Nonce, MessageState>,
            Vec<TxId>,
        ),
        Error,
    > {
        let mut collided: Vec<TxId> = Vec::new();
        // iterate over all inputs and check for collision
        let mut max_depth = 0;
        let mut db_coins: HashMap<UtxoId, CoinState> = HashMap::new();
        let mut db_contracts: HashMap<ContractId, ContractState> = HashMap::new();
        let mut db_messages: HashMap<Nonce, MessageState> = HashMap::new();
        for input in tx.inputs() {
            // check if all required inputs are here.
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                    // is it dependent output?
                    if let Some(state) = self.coins.get(utxo_id) {
                        // check depth
                        max_depth =
                            core::cmp::max(state.depth.saturating_add(1), max_depth);
                        if max_depth > self.max_depth {
                            return Err(Error::NotInsertedMaxDepth)
                        }
                        // output is present but is it spend by other tx?
                        if let Some(ref spend_by) = state.is_spend_by {
                            // get tx that is spending this output
                            let txpool_tx = txs
                                .get(spend_by)
                                .expect("Tx should be always present in txpool");
                            // compare if tx has better price
                            if txpool_tx.price() > tx.price() {
                                return Err(Error::NotInsertedCollision(
                                    *spend_by, *utxo_id,
                                ))
                            } else {
                                if state.is_in_database() {
                                    // this means it is loaded from db. Get tx to compare output.
                                    if self.utxo_validation {
                                        let coin = db
                                            .utxo(utxo_id)
                                            .map_err(|e| {
                                                Error::Database(format!("{:?}", e))
                                            })?
                                            .ok_or(
                                                Error::NotInsertedInputUtxoIdNotDoesNotExist(
                                                    *utxo_id,
                                                ),
                                            )?;
                                        if !coin
                                            .matches_input(input)
                                            .expect("The input is coin above")
                                        {
                                            return Err(Error::NotInsertedIoCoinMismatch)
                                        }
                                    }
                                } else {
                                    // tx output is in pool
                                    let output_tx = txs.get(utxo_id.tx_id()).unwrap();
                                    let output = &output_tx.outputs()
                                        [utxo_id.output_index() as usize];
                                    Self::check_if_coin_input_can_spend_output(
                                        output, input, false,
                                    )?;
                                };

                                collided.push(*spend_by);
                            }
                        }
                        // if coin is not spend, it will be spend later down the line
                    } else {
                        if self.utxo_validation {
                            // fetch from db and check if tx exist.
                            let coin = db
                                .utxo(utxo_id)
                                .map_err(|e| Error::Database(format!("{:?}", e)))?
                                .ok_or(Error::NotInsertedInputUtxoIdNotDoesNotExist(
                                    *utxo_id,
                                ))?;

                            if !coin
                                .matches_input(input)
                                .expect("The input is coin above")
                            {
                                return Err(Error::NotInsertedIoCoinMismatch)
                            }
                        }
                        max_depth = core::cmp::max(1, max_depth);
                    }
                    // mark this coin as spent by the current tx
                    db_coins.insert(
                        *utxo_id,
                        CoinState {
                            is_spend_by: Some(tx.id() as TxId),
                            depth: max_depth
                                .checked_sub(1)
                                .expect("The `max_depth` is always more than zero above"),
                        },
                    );
                    // yey we got our coin
                }
                Input::MessageCoinSigned(MessageCoinSigned { nonce, .. })
                | Input::MessageCoinPredicate(MessageCoinPredicate { nonce, .. })
                | Input::MessageDataSigned(MessageDataSigned { nonce, .. })
                | Input::MessageDataPredicate(MessageDataPredicate { nonce, .. }) => {
                    // since message id is derived, we don't need to double check all the fields
                    if self.utxo_validation {
                        if let Some(db_message) = db
                            .message(nonce)
                            .map_err(|e| Error::Database(format!("{:?}", e)))?
                        {
                            // verify message id integrity
                            if !db_message
                                .matches_input(input)
                                .expect("Input is a message above")
                            {
                                return Err(Error::NotInsertedIoMessageMismatch)
                            }
                            // return an error if spent block is set
                            if db
                                .is_message_spent(nonce)
                                .map_err(|e| Error::Database(format!("{:?}", e)))?
                            {
                                return Err(Error::NotInsertedInputMessageSpent(*nonce))
                            }
                        } else {
                            return Err(Error::NotInsertedInputMessageUnknown(*nonce))
                        }
                    }

                    if let Some(state) = self.messages.get(nonce) {
                        // some other is already attempting to spend this message, compare gas price
                        if state.gas_price >= tx.price() {
                            return Err(Error::NotInsertedCollisionMessageId(
                                state.spent_by,
                                *nonce,
                            ))
                        } else {
                            collided.push(state.spent_by);
                        }
                    }
                    db_messages.insert(
                        *nonce,
                        MessageState {
                            spent_by: tx.id(),
                            gas_price: tx.price(),
                        },
                    );
                }
                Input::Contract(Contract { contract_id, .. }) => {
                    // Does contract exist. We don't need to do any check here other then if contract_id exist or not.
                    if let Some(state) = self.contracts.get(contract_id) {
                        // check if contract is created after this transaction.
                        if tx.price() > state.gas_price {
                            return Err(Error::NotInsertedContractPricedLower(
                                *contract_id,
                            ))
                        }
                        // check depth.
                        max_depth = core::cmp::max(state.depth, max_depth);
                        if max_depth > self.max_depth {
                            return Err(Error::NotInsertedMaxDepth)
                        }
                    } else {
                        if !db
                            .contract_exist(contract_id)
                            .map_err(|e| Error::Database(format!("{:?}", e)))?
                        {
                            return Err(Error::NotInsertedInputContractDoesNotExist(
                                *contract_id,
                            ))
                        }
                        // add depth
                        max_depth = core::cmp::max(1, max_depth);
                        // insert into contract
                        db_contracts
                            .entry(*contract_id)
                            .or_insert(ContractState {
                                used_by: HashSet::new(),
                                depth: 0,
                                origin: None, // there is no owner if contract is in db
                                gas_price: GasPrice::MAX,
                            })
                            .used_by
                            .insert(tx.id());
                    }

                    // yey we got our contract
                }
            }
        }

        // nice, our inputs don't collide. Now check if our newly created contracts collide on ContractId
        for output in tx.outputs() {
            if let Output::ContractCreated { contract_id, .. } = output {
                if let Some(contract) = self.contracts.get(contract_id) {
                    // we have a collision :(
                    if contract.is_in_database() {
                        return Err(Error::NotInsertedContractIdAlreadyTaken(*contract_id))
                    }
                    // check who is priced more
                    if contract.gas_price > tx.price() {
                        // new tx is priced less then current tx
                        return Err(Error::NotInsertedCollisionContractId(*contract_id))
                    }
                    // if we are prices more, mark current contract origin for removal.
                    let origin = contract.origin.expect(
                        "Only contract without origin are the ones that are inside DB. And we check depth for that, so we are okay to just unwrap"
                        );
                    collided.push(*origin.tx_id());
                }
            }
            // collision of other outputs is not possible.
        }

        Ok((max_depth, db_coins, db_contracts, db_messages, collided))
    }

    /// insert tx inside dependency
    /// return list of transactions that are removed from txpool
    pub(crate) fn insert<'a, DB>(
        &'a mut self,
        txs: &'a HashMap<TxId, TxInfo>,
        db: &DB,
        tx: &'a ArcPoolTx,
    ) -> Result<Vec<ArcPoolTx>, Error>
    where
        DB: TxPoolDb,
    {
        let (max_depth, db_coins, db_contracts, db_messages, collided) =
            self.check_for_collision(txs, db, tx)?;

        // now we are sure that transaction can be included. remove all collided transactions
        let mut removed_tx = Vec::new();
        for collided in collided.into_iter() {
            let collided = txs
                .get(&collided)
                .expect("Collided should be present in txpool");
            removed_tx.extend(
                self.recursively_remove_all_dependencies(txs, collided.tx().clone()),
            );
        }

        // iterate over all inputs and spend parent coins/contracts
        for input in tx.inputs() {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                    // spend coin
                    if let Some(state) = self.coins.get_mut(utxo_id) {
                        state.is_spend_by = Some(tx.id());
                    }
                }
                Input::Contract(Contract { contract_id, .. }) => {
                    // Contract that we want to use can be already inside dependency (this case)
                    // or it will be added when db_contracts extends self.contracts (and it
                    // already contains changed used_by)
                    if let Some(state) = self.contracts.get_mut(contract_id) {
                        state.used_by.insert(tx.id());
                    }
                }
                Input::MessageCoinSigned(_)
                | Input::MessageCoinPredicate(_)
                | Input::MessageDataSigned(_)
                | Input::MessageDataPredicate(_) => {}
            }
        }

        // insert all coins/contracts that we got from db;
        self.coins.extend(db_coins);
        // for contracts from db that are not found in dependency, we already inserted used_by
        // and are okay to just extend current list
        self.contracts.extend(db_contracts);
        // insert / overwrite all applicable message id spending relations
        self.messages.extend(db_messages);

        // iterate over all outputs and insert them, marking them as available.
        for (index, output) in tx.outputs().iter().enumerate() {
            let index = u8::try_from(index).map_err(|_| {
                Error::Other(format!(
                    "The number of outputs in `{}` is more than `u8::max`",
                    tx.id()
                ))
            })?;
            let utxo_id = UtxoId::new(tx.id(), index);
            match output {
                Output::Coin { .. } | Output::Change { .. } | Output::Variable { .. } => {
                    // insert output coin inside by_coin
                    self.coins.insert(
                        utxo_id,
                        CoinState {
                            is_spend_by: None,
                            depth: max_depth,
                        },
                    );
                }
                Output::ContractCreated { contract_id, .. } => {
                    // insert contract
                    self.contracts.insert(
                        *contract_id,
                        ContractState {
                            depth: max_depth,
                            used_by: HashSet::new(),
                            origin: Some(utxo_id),
                            gas_price: tx.price(),
                        },
                    );
                }
                Output::Contract(_) => {
                    // do nothing, this contract is already already found in dependencies.
                    // as it is tied with input and used_by is already inserted.
                }
            };
        }

        Ok(removed_tx)
    }

    /// Remove all pending txs that depend on the outputs of the provided tx
    pub(crate) fn recursively_remove_all_dependencies<'a>(
        &'a mut self,
        txs: &'a HashMap<TxId, TxInfo>,
        tx: ArcPoolTx,
    ) -> Vec<ArcPoolTx> {
        let mut removed_transactions = vec![tx.clone()];

        // recursively remove all transactions that depend on the outputs of the current tx
        for (index, output) in tx.outputs().iter().enumerate() {
            match output {
                Output::Contract(_) => {
                    // no other transactions can depend on these types of outputs
                }
                Output::Coin { .. } | Output::Change { .. } | Output::Variable { .. } => {
                    let index = u8::try_from(index)
                        .expect("The number of outputs is more than `u8::max`. \
                        But it should be impossible because we don't include transactions with so many outputs.");
                    // remove transactions that depend on this coin output
                    let utxo = UtxoId::new(tx.id(), index);
                    if let Some(state) = self.coins.remove(&utxo).map(|c| c.is_spend_by) {
                        // there may or may not be any dependents for this coin output
                        if let Some(spend_by) = state {
                            let rem_tx =
                                txs.get(&spend_by).expect("Tx should be present in txs");
                            removed_transactions.extend(
                                self.recursively_remove_all_dependencies(
                                    txs,
                                    rem_tx.tx().clone(),
                                ),
                            );
                        }
                    } else {
                        // this shouldn't happen since the output belongs to this unique transaction,
                        // no other resources should remove this coin state except this transaction.
                        warn!("expected a coin state to be associated with {:?}", &utxo);
                    }
                }
                Output::ContractCreated { contract_id, .. } => {
                    // remove any other pending txs that depend on our yet to be created contract
                    if let Some(contract) = self.contracts.remove(contract_id) {
                        for spend_by in contract.used_by {
                            let rem_tx =
                                txs.get(&spend_by).expect("Tx should be present in txs");
                            removed_transactions.extend(
                                self.recursively_remove_all_dependencies(
                                    txs,
                                    rem_tx.tx().clone(),
                                ),
                            );
                        }
                    }
                }
            };
        }

        // remove this transaction as a dependency of its' inputs.
        for input in tx.inputs() {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                    // Input coin cases
                    // 1. coin state was already removed if parent tx was also removed, no cleanup required.
                    // 2. coin state spent_by needs to be freed from this tx if parent tx isn't being removed
                    // 3. coin state can be removed if this is a database coin, as no other txs are involved.
                    if let Some(state) = self.coins.get_mut(utxo_id) {
                        if !state.is_in_database() {
                            // case 2
                            state.is_spend_by = None;
                        } else {
                            // case 3
                            self.coins.remove(utxo_id);
                        }
                    }
                }
                Input::Contract(Contract { contract_id, .. }) => {
                    // Input contract cases
                    // 1. contract state no longer exists because the parent tx that created the
                    //    contract was already removed from the graph
                    // 2. contract state exists and this tx needs to be removed as a user of it.
                    // 2.a. contract state can be removed if it's from the database and this is the
                    //      last tx to use it, since no other txs are involved.
                    if let Some(state) = self.contracts.get_mut(contract_id) {
                        state.used_by.remove(&tx.id());
                        // if contract list is empty and is in db, flag contract state for removal.
                        if state.used_by.is_empty() && state.is_in_database() {
                            self.contracts.remove(contract_id);
                        }
                    }
                }
                Input::MessageCoinSigned(MessageCoinSigned { nonce, .. })
                | Input::MessageCoinPredicate(MessageCoinPredicate { nonce, .. })
                | Input::MessageDataSigned(MessageDataSigned { nonce, .. })
                | Input::MessageDataPredicate(MessageDataPredicate { nonce, .. }) => {
                    self.messages.remove(nonce);
                }
            }
        }

        removed_transactions
    }
}

#[cfg(test)]
mod tests {

    use std::str::FromStr;

    use fuel_core_types::{
        fuel_tx::UtxoId,
        fuel_types::{
            Address,
            AssetId,
            Bytes32,
        },
    };

    use super::*;

    #[test]
    fn test_check_between_input_output() {
        use fuel_core_txpool as _;

        let output = Output::Coin {
            to: Address::default(),
            amount: 10,
            asset_id: AssetId::default(),
        };
        let input = Input::coin_signed(
            UtxoId::default(),
            Address::default(),
            10,
            AssetId::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        let out =
            Dependency::check_if_coin_input_can_spend_output(&output, &input, false);
        assert!(out.is_ok(), "test1. It should be Ok");

        let output = Output::Coin {
            to: Address::default(),
            amount: 12,
            asset_id: AssetId::default(),
        };

        let out =
            Dependency::check_if_coin_input_can_spend_output(&output, &input, false);
        assert!(out.is_err(), "test2 There should be error");
        assert!(
            matches!(out.err().unwrap(), Error::NotInsertedIoWrongAmount),
            "test2"
        );

        let output = Output::Coin {
            to: Address::from_str(
                "0x0000000000000000000000000000000000000000000000000000000000000001",
            )
            .unwrap(),
            amount: 10,
            asset_id: AssetId::default(),
        };

        let out =
            Dependency::check_if_coin_input_can_spend_output(&output, &input, false);
        assert!(out.is_err(), "test3 There should be error");
        assert!(
            matches!(out.err().unwrap(), Error::NotInsertedIoWrongOwner),
            "test3"
        );

        let output = Output::Coin {
            to: Address::default(),
            amount: 10,
            asset_id: AssetId::from_str(
                "0x0000000000000000000000000000000000000000000000000000000000000001",
            )
            .unwrap(),
        };

        let out =
            Dependency::check_if_coin_input_can_spend_output(&output, &input, false);
        assert!(out.is_err(), "test4 There should be error");
        assert!(
            matches!(out.err().unwrap(), Error::NotInsertedIoWrongAssetId),
            "test4"
        );

        let output = Output::Coin {
            to: Address::default(),
            amount: 10,
            asset_id: AssetId::from_str(
                "0x0000000000000000000000000000000000000000000000000000000000000001",
            )
            .unwrap(),
        };

        let out =
            Dependency::check_if_coin_input_can_spend_output(&output, &input, false);
        assert!(out.is_err(), "test5 There should be error");
        assert!(
            matches!(out.err().unwrap(), Error::NotInsertedIoWrongAssetId),
            "test5"
        );

        let output = Output::contract(0, Default::default(), Default::default());

        let out =
            Dependency::check_if_coin_input_can_spend_output(&output, &input, false);
        assert!(out.is_err(), "test6 There should be error");
        assert!(
            matches!(out.err().unwrap(), Error::NotInsertedIoContractOutput),
            "test6"
        );

        let output = Output::ContractCreated {
            contract_id: ContractId::default(),
            state_root: Bytes32::default(),
        };

        let out =
            Dependency::check_if_coin_input_can_spend_output(&output, &input, false);
        assert!(out.is_err(), "test8 There should be error");
        assert!(
            matches!(out.err().unwrap(), Error::NotInsertedIoContractOutput),
            "test7"
        );
    }
}
