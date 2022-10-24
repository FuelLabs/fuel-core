use crate::{
    types::*,
    Error,
};
use anyhow::anyhow;
use fuel_core_interfaces::{
    common::{
        fuel_tx::{
            Input,
            Output,
            UtxoId,
        },
        fuel_types::MessageId,
    },
    model::{
        ArcTx,
        Coin,
        CoinStatus,
        TxInfo,
    },
    txpool::TxPoolDb,
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
    messages: HashMap<MessageId, MessageState>,
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
        tx: ArcTx,
        seen: &mut HashMap<TxId, ArcTx>,
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
                        Input::CoinSigned { utxo_id, .. }
                        | Input::CoinPredicate { utxo_id, .. } => {
                            let state = self
                                .coins
                                .get(utxo_id)
                                .expect("to find coin inside spend tx");
                            if !state.is_in_database() {
                                check.push(*utxo_id.tx_id());
                            }
                        }
                        Input::Contract { contract_id, .. } => {
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
                        Input::MessageSigned { .. } | Input::MessagePredicate { .. } => {
                            // Message inputs do not depend on any other fuel transactions
                        }
                    }
                }
            }
        }
    }

    fn check_if_coin_input_can_spend_db_coin(
        coin: &Coin,
        input: &Input,
    ) -> anyhow::Result<()> {
        match input {
            Input::CoinSigned {
                utxo_id,
                owner,
                amount,
                asset_id,
                ..
            }
            | Input::CoinPredicate {
                utxo_id,
                owner,
                amount,
                asset_id,
                ..
            } => {
                if *owner != coin.owner {
                    return Err(Error::NotInsertedIoWrongOwner.into())
                }
                if *amount != coin.amount {
                    return Err(Error::NotInsertedIoWrongAmount.into())
                }
                if *asset_id != coin.asset_id {
                    return Err(Error::NotInsertedIoWrongAssetId.into())
                }
                if coin.status == CoinStatus::Spent {
                    return Err(Error::NotInsertedInputUtxoIdSpent(*utxo_id).into())
                }
                Ok(())
            }
            _ => Err(anyhow!("Use it only for coin output check")),
        }
    }

    fn check_if_coin_input_can_spend_output(
        output: &Output,
        input: &Input,
        is_output_filled: bool,
    ) -> anyhow::Result<()> {
        if let Input::CoinSigned {
            owner,
            amount,
            asset_id,
            ..
        }
        | Input::CoinPredicate {
            owner,
            amount,
            asset_id,
            ..
        } = input
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
                        return Err(Error::NotInsertedIoWrongOwner.into())
                    }
                    if amount != i_amount {
                        return Err(Error::NotInsertedIoWrongAmount.into())
                    }
                    if asset_id != i_asset_id {
                        return Err(Error::NotInsertedIoWrongAssetId.into())
                    }
                }
                Output::Contract { .. } => {
                    return Err(Error::NotInsertedIoContractOutput.into())
                }
                Output::Message { .. } => {
                    return Err(Error::NotInsertedIoMessageInput.into())
                }
                Output::Change {
                    to,
                    asset_id,
                    amount,
                } => {
                    if to != i_owner {
                        return Err(Error::NotInsertedIoWrongOwner.into())
                    }
                    if asset_id != i_asset_id {
                        return Err(Error::NotInsertedIoWrongAssetId.into())
                    }
                    if is_output_filled && amount != i_amount {
                        return Err(Error::NotInsertedIoWrongAmount.into())
                    }
                }
                Output::Variable {
                    to,
                    amount,
                    asset_id,
                } => {
                    if is_output_filled {
                        if to != i_owner {
                            return Err(Error::NotInsertedIoWrongOwner.into())
                        }
                        if amount != i_amount {
                            return Err(Error::NotInsertedIoWrongAmount.into())
                        }
                        if asset_id != i_asset_id {
                            return Err(Error::NotInsertedIoWrongAssetId.into())
                        }
                    }
                    // else do nothing, everything is variable and can be only check on execution
                }
                Output::ContractCreated { .. } => {
                    return Err(Error::NotInsertedIoContractOutput.into())
                }
            };
        } else {
            return Err(anyhow!("Use it only for coin output check"))
        }
        Ok(())
    }

    /// Verifies the integrity of the message ID
    fn check_if_message_input_matches_id(input: &Input) -> anyhow::Result<()> {
        match input {
            Input::MessageSigned {
                message_id,
                sender,
                recipient,
                nonce,
                amount,
                data,
                ..
            }
            | Input::MessagePredicate {
                message_id,
                sender,
                recipient,
                nonce,
                amount,
                data,
                ..
            } => {
                let computed_id =
                    Input::compute_message_id(sender, recipient, *nonce, *amount, data);
                if message_id != &computed_id {
                    return Err(Error::NotInsertedIoWrongMessageId.into())
                }
            }
            _ => {}
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
        tx: &'a ArcTx,
    ) -> anyhow::Result<(
        usize,
        HashMap<UtxoId, CoinState>,
        HashMap<ContractId, ContractState>,
        HashMap<MessageId, MessageState>,
        Vec<TxId>,
    )> {
        let mut collided: Vec<TxId> = Vec::new();
        // iterate over all inputs and check for collision
        let mut max_depth = 0;
        let mut db_coins: HashMap<UtxoId, CoinState> = HashMap::new();
        let mut db_contracts: HashMap<ContractId, ContractState> = HashMap::new();
        let mut db_messages: HashMap<MessageId, MessageState> = HashMap::new();
        for input in tx.inputs() {
            // check if all required inputs are here.
            match input {
                Input::CoinSigned { utxo_id, .. }
                | Input::CoinPredicate { utxo_id, .. } => {
                    // is it dependent output?
                    if let Some(state) = self.coins.get(utxo_id) {
                        // check depth
                        max_depth = core::cmp::max(state.depth + 1, max_depth);
                        if max_depth > self.max_depth {
                            return Err(Error::NotInsertedMaxDepth.into())
                        }
                        // output is present but is it spend by other tx?
                        if let Some(ref spend_by) = state.is_spend_by {
                            // get tx that is spending this output
                            let txpool_tx = txs
                                .get(spend_by)
                                .expect("Tx should be always present in txpool");
                            // compare if tx has better price
                            if txpool_tx.gas_price() > tx.gas_price() {
                                return Err(Error::NotInsertedCollision(
                                    *spend_by, *utxo_id,
                                )
                                .into())
                            } else {
                                if state.is_in_database() {
                                    // this means it is loaded from db. Get tx to compare output.
                                    if self.utxo_validation {
                                        let coin = db.utxo(utxo_id)?.ok_or(
                                            Error::NotInsertedInputUtxoIdNotExisting(
                                                *utxo_id,
                                            ),
                                        )?;
                                        Self::check_if_coin_input_can_spend_db_coin(
                                            &coin, input,
                                        )?;
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
                            let coin = db.utxo(utxo_id)?.ok_or(
                                Error::NotInsertedInputUtxoIdNotExisting(*utxo_id),
                            )?;

                            Self::check_if_coin_input_can_spend_db_coin(&coin, input)?;
                        }

                        max_depth = core::cmp::max(1, max_depth);
                        db_coins.insert(
                            *utxo_id,
                            CoinState {
                                is_spend_by: Some(tx.id() as TxId),
                                depth: 0,
                            },
                        );
                    }

                    // yey we got our coin
                }
                Input::MessagePredicate { message_id, .. }
                | Input::MessageSigned { message_id, .. } => {
                    // verify message id integrity
                    Self::check_if_message_input_matches_id(input)?;
                    // since message id is derived, we don't need to double check all the fields
                    if self.utxo_validation {
                        if let Some(msg) = db.message(message_id)? {
                            // return an error if spent block is set
                            if msg.fuel_block_spend.is_some() {
                                return Err(Error::NotInsertedInputMessageIdSpent(
                                    *message_id,
                                )
                                .into())
                            }
                        } else {
                            return Err(
                                Error::NotInsertedInputMessageUnknown(*message_id).into()
                            )
                        }
                    }

                    if let Some(state) = self.messages.get(message_id) {
                        // some other is already attempting to spend this message, compare gas price
                        if state.gas_price >= tx.gas_price() {
                            return Err(Error::NotInsertedCollisionMessageId(
                                state.spent_by,
                                *message_id,
                            )
                            .into())
                        } else {
                            collided.push(state.spent_by);
                        }
                    }
                    db_messages.insert(
                        *message_id,
                        MessageState {
                            spent_by: tx.id(),
                            gas_price: tx.gas_price(),
                        },
                    );
                }
                Input::Contract { contract_id, .. } => {
                    // Does contract exist. We don't need to do any check here other then if contract_id exist or not.
                    if let Some(state) = self.contracts.get(contract_id) {
                        // check if contract is created after this transaction.
                        if tx.gas_price() > state.gas_price {
                            return Err(Error::NotInsertedContractPricedLower(
                                *contract_id,
                            )
                            .into())
                        }
                        // check depth.
                        max_depth = core::cmp::max(state.depth, max_depth);
                        if max_depth > self.max_depth {
                            return Err(Error::NotInsertedMaxDepth.into())
                        }
                    } else {
                        if !db.contract_exist(contract_id)? {
                            return Err(Error::NotInsertedInputContractNotExisting(
                                *contract_id,
                            )
                            .into())
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
                        return Err(
                            Error::NotInsertedContractIdAlreadyTaken(*contract_id).into()
                        )
                    }
                    // check who is priced more
                    if contract.gas_price > tx.gas_price() {
                        // new tx is priced less then current tx
                        return Err(
                            Error::NotInsertedCollisionContractId(*contract_id).into()
                        )
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
    pub(crate) async fn insert<'a>(
        &'a mut self,
        txs: &'a HashMap<TxId, TxInfo>,
        db: &dyn TxPoolDb,
        tx: &'a ArcTx,
    ) -> anyhow::Result<Vec<ArcTx>> {
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
                Input::CoinSigned { utxo_id, .. }
                | Input::CoinPredicate { utxo_id, .. } => {
                    // spend coin
                    if let Some(state) = self.coins.get_mut(utxo_id) {
                        state.is_spend_by = Some(tx.id());
                    }
                }
                Input::Contract { contract_id, .. } => {
                    // Contract that we want to use can be already inside dependency (this case)
                    // or it will be added when db_contracts extends self.contracts (and it
                    // already contains changed used_by)
                    if let Some(state) = self.contracts.get_mut(contract_id) {
                        state.used_by.insert(tx.id());
                    }
                }
                Input::MessageSigned { .. } | Input::MessagePredicate { .. } => {}
            }
        }

        // insert all coins/contracts that we got from db;
        self.coins.extend(db_coins.into_iter());
        // for contracts from db that are not found in dependency, we already inserted used_by
        // and are okay to just extend current list
        self.contracts.extend(db_contracts.into_iter());
        // insert / overwrite all applicable message id spending relations
        self.messages.extend(db_messages.into_iter());

        // iterate over all outputs and insert them, marking them as available.
        for (index, output) in tx.outputs().iter().enumerate() {
            let utxo_id = UtxoId::new(tx.id(), index as u8);
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
                            gas_price: tx.gas_price(),
                        },
                    );
                }
                Output::Message { .. } => {
                    // withdrawal does nothing and it should not be found in dependency.
                }
                Output::Contract { .. } => {
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
        tx: ArcTx,
    ) -> Vec<ArcTx> {
        let mut removed_transactions = vec![tx.clone()];

        // recursively remove all transactions that depend on the outputs of the current tx
        for (index, output) in tx.outputs().iter().enumerate() {
            match output {
                Output::Message { .. } | Output::Contract { .. } => {
                    // no other transactions can depend on these types of outputs
                }
                Output::Coin { .. } | Output::Change { .. } | Output::Variable { .. } => {
                    // remove transactions that depend on this coin output
                    let utxo = UtxoId::new(tx.id(), index as u8);
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
                Input::CoinSigned { utxo_id, .. }
                | Input::CoinPredicate { utxo_id, .. } => {
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
                Input::Contract { contract_id, .. } => {
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
                Input::MessageSigned { message_id, .. }
                | Input::MessagePredicate { message_id, .. } => {
                    self.messages.remove(message_id);
                }
            }
        }

        removed_transactions
    }
}

#[cfg(test)]
mod tests {

    use std::str::FromStr;

    use fuel_core_interfaces::common::{
        fuel_tx::{
            Address,
            AssetId,
            UtxoId,
        },
        fuel_types::Bytes32,
    };

    use super::*;

    #[test]
    fn test_check_between_input_output() {
        let output = Output::Coin {
            to: Address::default(),
            amount: 10,
            asset_id: AssetId::default(),
        };
        let input = Input::CoinSigned {
            utxo_id: UtxoId::default(),
            owner: Address::default(),
            amount: 10,
            asset_id: AssetId::default(),
            tx_pointer: Default::default(),
            witness_index: 0,
            maturity: 0,
        };
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
        assert_eq!(
            out.err().unwrap().downcast_ref(),
            Some(&Error::NotInsertedIoWrongAmount),
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
        assert_eq!(
            out.err().unwrap().downcast_ref(),
            Some(&Error::NotInsertedIoWrongOwner),
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
        assert_eq!(
            out.err().unwrap().downcast_ref(),
            Some(&Error::NotInsertedIoWrongAssetId),
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
        assert_eq!(
            out.err().unwrap().downcast_ref(),
            Some(&Error::NotInsertedIoWrongAssetId),
            "test5"
        );

        let output = Output::Contract {
            input_index: 0,
            balance_root: Default::default(),
            state_root: Default::default(),
        };

        let out =
            Dependency::check_if_coin_input_can_spend_output(&output, &input, false);
        assert!(out.is_err(), "test6 There should be error");
        assert_eq!(
            out.err().unwrap().downcast_ref(),
            Some(&Error::NotInsertedIoContractOutput),
            "test6"
        );

        let output = Output::Message {
            recipient: Default::default(),
            amount: 0,
        };

        let out =
            Dependency::check_if_coin_input_can_spend_output(&output, &input, false);
        assert!(out.is_err(), "test7 There should be error");
        assert_eq!(
            out.err().unwrap().downcast_ref(),
            Some(&Error::NotInsertedIoMessageInput),
            "test7"
        );

        let output = Output::ContractCreated {
            contract_id: ContractId::default(),
            state_root: Bytes32::default(),
        };

        let out =
            Dependency::check_if_coin_input_can_spend_output(&output, &input, false);
        assert!(out.is_err(), "test8 There should be error");
        assert_eq!(
            out.err().unwrap().downcast_ref(),
            Some(&Error::NotInsertedIoContractOutput),
            "test8"
        );
    }
}
