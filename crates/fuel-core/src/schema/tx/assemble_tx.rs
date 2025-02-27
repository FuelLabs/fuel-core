use crate::{
    coins_query::CoinsQueryError,
    fuel_core_graphql_api::{
        api_service::BlockProducer,
        database::ReadView,
        ports::MemoryPool,
    },
    query::asset_query::Exclude,
    schema::{
        coins::{
            CoinType,
            SpendQueryElementInput,
        },
        tx::{
            Account,
            ChangePolicy,
            RequiredBalance,
        },
    },
    service::adapters::SharedMemoryPool,
};
use fuel_core_types::{
    entities::coins::CoinId,
    fuel_asm::{
        PanicReason,
        Word,
    },
    fuel_crypto::Signature,
    fuel_tx::{
        field::{
            Inputs,
            MaxFeeLimit,
            Outputs,
            Script as ScriptField,
            ScriptGasLimit,
            WitnessLimit,
        },
        input::{
            coin::CoinSigned,
            message::{
                MessageCoinSigned,
                MessageDataSigned,
            },
        },
        Address,
        AssetId,
        Chargeable,
        ConsensusParameters,
        Input,
        Output,
        Receipt,
        Script,
    },
    fuel_types::canonical::Serialize,
    fuel_vm::{
        checked_transaction::CheckPredicateParams,
        interpreter::ExecutableTransaction,
    },
    services::executor::{
        TransactionExecutionResult,
        TransactionExecutionStatus,
    },
};
use std::{
    collections::{
        hash_map::Entry,
        HashMap,
        HashSet,
    },
    sync::Arc,
};

const FAKE_AMOUNT: u64 = 1_000_000_000_000;

pub struct AssembleArguments<'a> {
    pub fee_index: u16,
    pub required_balances: Vec<RequiredBalance>,
    pub exclude: Exclude,
    pub estimate_predicates: bool,
    pub reserve_gas: u64,
    pub consensus_parameters: Arc<ConsensusParameters>,
    pub gas_price: u64,
    pub read_view: Arc<ReadView>,
    pub block_producer: &'a BlockProducer,
    pub shared_memory_pool: &'a SharedMemoryPool,
}

impl<'a> AssembleArguments<'a> {
    async fn coins(
        &self,
        owner: Address,
        asset_id: AssetId,
        amount: u64,
        remaining_input_slots: u16,
    ) -> anyhow::Result<Vec<CoinType>> {
        let query_per_asset = SpendQueryElementInput {
            asset_id: asset_id.into(),
            amount: (amount as u128).into(),
            max: None,
        };

        let result = self
            .read_view
            .coins_to_spend(
                owner,
                &[query_per_asset],
                &self.exclude,
                &self.consensus_parameters,
                remaining_input_slots,
            )
            .await?
            .into_iter()
            .next()
            .expect("The query returns a single result; qed");

        Ok(result)
    }

    async fn dry_run(
        &self,
        script: Script,
    ) -> anyhow::Result<TransactionExecutionStatus> {
        self.block_producer
            .dry_run_txs(
                vec![script.into()],
                None,
                None,
                Some(false),
                Some(self.gas_price),
            )
            .await?
            .into_iter()
            .next()
            .ok_or(anyhow::anyhow!("No result for the dry run"))
    }
}

pub struct AssembleTx<'a, Tx> {
    tx: Tx,
    arguments: AssembleArguments<'a>,
    signature_witness_indexes: HashMap<Address, u16>,
    change_output_policies: HashMap<AssetId, ChangePolicy>,
    set_change_outputs: HashSet<AssetId>,
    added_fake_coin: bool,
    // The amount of the base asset that is reserved for the application logic
    base_asset_reserved: u64,
    fee_payer_account: Account,
}

impl<'a, Tx> AssembleTx<'a, Tx>
where
    Tx: ExecutableTransaction + Send + 'static,
{
    pub fn new(tx: Tx, mut arguments: AssembleArguments<'a>) -> anyhow::Result<Self> {
        if arguments.fee_index as usize >= arguments.required_balances.len() {
            return Err(anyhow::anyhow!("The fee address index is out of bounds").into());
        }

        let mut duplicate_checker =
            HashSet::with_capacity(arguments.required_balances.len());
        for required_balance in &arguments.required_balances {
            let asset_id = required_balance.asset_id;
            if !duplicate_checker.insert(asset_id) {
                return Err(CoinsQueryError::DuplicateAssets(asset_id).into());
            }
        }

        let mut signature_witness_indexes = HashMap::<Address, u16>::new();

        // Exclude inputs that already are used by the transaction
        let inputs = tx.inputs();
        for input in inputs {
            if let Some(utxo_id) = input.utxo_id() {
                arguments.exclude.exclude(CoinId::Utxo(*utxo_id));
            }

            if let Some(nonce) = input.nonce() {
                arguments.exclude.exclude(CoinId::Message(*nonce));
            }

            match input {
                Input::CoinSigned(CoinSigned {
                    owner,
                    witness_index,
                    ..
                })
                | Input::MessageCoinSigned(MessageCoinSigned {
                    recipient: owner,
                    witness_index,
                    ..
                })
                | Input::MessageDataSigned(MessageDataSigned {
                    recipient: owner,
                    witness_index,
                    ..
                }) => {
                    signature_witness_indexes.insert(*owner, *witness_index);
                }
                Input::Contract(_)
                | Input::CoinPredicate(_)
                | Input::MessageCoinPredicate(_)
                | Input::MessageDataPredicate(_) => {
                    // Do nothing
                }
            }
        }

        let mut change_output_policies = HashMap::<AssetId, ChangePolicy>::new();
        let mut set_change_outputs = HashSet::<AssetId>::new();

        for output in tx.outputs() {
            if let Output::Change { to, asset_id, .. } = output {
                change_output_policies
                    .insert(*asset_id, ChangePolicy::Change((*to).into()));
                set_change_outputs.insert(*asset_id);
            }
        }

        let mut base_asset_reserved = None;
        let base_asset_id = *arguments.consensus_parameters.base_asset_id();

        for required_balance in &arguments.required_balances {
            let asset_id = required_balance.asset_id;

            if asset_id == base_asset_id {
                base_asset_reserved = Some(required_balance.amount);
            }
        }

        let fee_payer_account = arguments.required_balances[arguments.fee_index as usize]
            .account
            .clone();

        // If the user didn't provide the base asset, we add it to the required balances
        // with minimal amount `1` and `ChangePolicy::Change` policy.
        if !base_asset_reserved.is_none() {
            let recipient = fee_payer_account.owner();

            arguments.required_balances.push(RequiredBalance {
                account: fee_payer_account.clone(),
                asset_id: base_asset_id,
                // `coins` query requires non-zero amount
                amount: 1,
                change_policy: ChangePolicy::Change(recipient),
            });
            base_asset_reserved = Some(0);
        }

        for required_balance in &arguments.required_balances {
            let asset_id = required_balance.asset_id;

            let entry = change_output_policies.entry(asset_id);

            match entry {
                Entry::Occupied(old) => {
                    if old.get() != &required_balance.change_policy {
                        return Err(anyhow::anyhow!(
                            "The asset {} has multiple change policies",
                            asset_id
                        )
                        .into());
                    }
                }
                Entry::Vacant(vacant) => {
                    vacant.insert(required_balance.change_policy);
                }
            }
        }

        let _self = Self {
            tx,
            arguments,
            signature_witness_indexes,
            change_output_policies,
            set_change_outputs,
            added_fake_coin: false,
            base_asset_reserved: base_asset_reserved.expect("Set above; qed"),
            fee_payer_account,
        };

        Ok(_self)
    }

    pub async fn assemble(mut self) -> anyhow::Result<Tx> {
        self.add_inputs_and_witnesses().await?;
        self.add_change_outputs();
        self.add_fake_coin()?;
        self.fill_with_variable_outputs();

        self.adjust_witness_limit();

        if self.arguments.estimate_predicates {
            self = self.estimate_predicates().await?;
        }

        self.estimate_script_if_possible().await?;

        self.remove_unused_variable_outputs();
        self.remove_fake_coin();

        self.cover_fee().await?;
        self.set_max_fee();

        Ok(self.tx)
    }

    fn remaining_input_slots(&self) -> anyhow::Result<u16> {
        let max_input = self.arguments.consensus_parameters.tx_params().max_inputs();
        let used_inputs = u16::try_from(self.tx.inputs().len()).unwrap_or(u16::MAX);

        if used_inputs > max_input {
            return Err(anyhow::anyhow!(
                "Filling required balances occupies a number \
                    of inputs more than can fit into the transaction"
            )
            .into());
        }

        Ok(max_input.saturating_sub(used_inputs))
    }

    async fn add_inputs_and_witnesses(&mut self) -> anyhow::Result<()> {
        let required_balance = core::mem::take(&mut self.arguments.required_balances);

        for required_balance in required_balance {
            let remaining_input_slots = self.remaining_input_slots()?;

            let asset_id = required_balance.asset_id;
            let amount = required_balance.amount;
            let owner = required_balance.account.owner();

            let selected_coins = self
                .arguments
                .coins(owner, asset_id, amount, remaining_input_slots)
                .await?;

            for coin in selected_coins {
                self.add_input_and_witness(&required_balance.account, coin);
            }
        }

        Ok(())
    }

    fn reserve_witness_index(&mut self, account: &Address) -> u16 {
        self.signature_witness_indexes
            .get(account)
            .cloned()
            .unwrap_or_else(|| {
                let vacant_index =
                    u16::try_from(self.tx.witnesses().len()).unwrap_or(u16::MAX);

                self.tx.witnesses_mut().push(vec![0; Signature::LEN].into());
                self.signature_witness_indexes
                    .insert(*account, vacant_index);
                vacant_index
            })
    }

    fn add_input_and_witness(&mut self, account: &Account, coin: CoinType) {
        let input = match account {
            Account::Address(account) => {
                let signature_index = self.reserve_witness_index(account);

                match coin {
                    CoinType::Coin(coin) => Input::coin_signed(
                        coin.0.utxo_id,
                        coin.0.owner,
                        coin.0.amount,
                        coin.0.asset_id,
                        coin.0.tx_pointer,
                        signature_index,
                    ),
                    CoinType::MessageCoin(message) => Input::message_coin_signed(
                        message.0.sender,
                        message.0.recipient,
                        message.0.amount,
                        message.0.nonce,
                        signature_index,
                    ),
                }
            }
            Account::Predicate(predicate) => {
                let predicate_gas_used = 0;
                match coin {
                    CoinType::Coin(coin) => Input::coin_predicate(
                        coin.0.utxo_id,
                        predicate.predicate_address,
                        coin.0.amount,
                        coin.0.asset_id,
                        coin.0.tx_pointer,
                        predicate_gas_used,
                        predicate.predicate.clone(),
                        predicate.predicate_data.clone(),
                    ),
                    CoinType::MessageCoin(message) => Input::message_coin_predicate(
                        message.0.sender,
                        message.0.recipient,
                        message.0.amount,
                        message.0.nonce,
                        predicate_gas_used,
                        predicate.predicate.clone(),
                        predicate.predicate_data.clone(),
                    ),
                }
            }
        };

        if let Some(utxo_id) = input.utxo_id() {
            self.arguments.exclude.exclude(CoinId::Utxo(*utxo_id));
        }

        if let Some(nonce) = input.nonce() {
            self.arguments.exclude.exclude(CoinId::Message(*nonce));
        }

        self.tx.inputs_mut().push(input);
    }

    fn add_change_outputs(&mut self) {
        let base_asset_id = *self.arguments.consensus_parameters.base_asset_id();
        let asset_ids = self
            .tx
            .inputs()
            .iter()
            .filter_map(|input| input.asset_id(&base_asset_id).cloned())
            .collect::<HashSet<AssetId>>();

        for asset_id in asset_ids {
            if self.set_change_outputs.insert(asset_id) {
                match self
                    .change_output_policies
                    .get(&asset_id)
                    .expect("Policy was inserted in the `new`; qed")
                {
                    ChangePolicy::Change(change_receiver) => {
                        self.tx.outputs_mut().push(Output::change(
                            *change_receiver,
                            0,
                            asset_id,
                        ));
                    }
                    ChangePolicy::Destroy => {
                        // Do nothing for now, since `fuel-tx` crate doesn't have
                        // `Destroy` output yet.
                        // https://github.com/FuelLabs/fuel-specs/issues/621
                    }
                }
            }
        }
    }

    fn add_fake_coin(&mut self) -> anyhow::Result<()> {
        if !self.is_runnable_script() {
            return Ok(())
        }

        let remaining_input_slots = self.remaining_input_slots()?;

        if remaining_input_slots == 0 {
            return Ok(())
        }

        let owner = self.arguments.required_balances[self.arguments.fee_index as usize]
            .account
            .owner();
        let asset_id = *self.arguments.consensus_parameters.base_asset_id();
        let witness_index = self.reserve_witness_index(&owner);

        let input = Input::coin_signed(
            Default::default(),
            owner,
            FAKE_AMOUNT,
            asset_id,
            Default::default(),
            witness_index,
        );

        self.tx.inputs_mut().push(input);

        self.added_fake_coin = true;

        Ok(())
    }

    fn remove_fake_coin(&mut self) {
        if !self.is_runnable_script() {
            return
        }

        if !self.added_fake_coin {
            return
        }

        self.tx.inputs_mut().pop();
        self.added_fake_coin = false;
    }

    fn is_runnable_script(&self) -> bool {
        if let Some(script) = self.tx.as_script() {
            if !script.script().is_empty() {
                return true
            }
        }
        false
    }

    // TODO: Optimize this function later to use information from the VM about missing
    //  `Variable` outputs.
    fn fill_with_variable_outputs(&mut self) {
        if !self.is_runnable_script() {
            return
        }

        let max_outputs = self
            .arguments
            .consensus_parameters
            .tx_params()
            .max_outputs();

        let outputs = u16::try_from(self.tx.outputs().len()).unwrap_or(u16::MAX);

        for _ in outputs..max_outputs {
            self.tx.outputs_mut().push(Output::variable(
                Default::default(),
                Default::default(),
                Default::default(),
            ));
        }
    }

    fn remove_unused_variable_outputs(&mut self) {
        if !self.is_runnable_script() {
            return
        }

        while let Some(output) = self.tx.outputs().last() {
            if let Output::Variable { amount, .. } = output {
                if *amount == 0 {
                    self.tx.outputs_mut().pop();
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }

    fn adjust_witness_limit(&mut self) {
        let witness_size = self.tx.witnesses().size_dynamic();
        self.tx.set_witness_limit(witness_size as u64);
    }

    async fn estimate_predicates(mut self) -> anyhow::Result<Self> {
        let memory = self.arguments.shared_memory_pool.get_memory().await;

        let parameters =
            CheckPredicateParams::from(self.arguments.consensus_parameters.as_ref());
        let read_view = self.arguments.read_view.clone();

        let mut tx_to_estimate = self.tx;
        let estimated_tx = tokio_rayon::spawn_fifo(move || {
            let result = tx_to_estimate.estimate_predicates(
                &parameters,
                memory,
                read_view.as_ref(),
            );
            result.map(|_| tx_to_estimate)
        })
        .await
        .map_err(|err| anyhow::anyhow!("{:?}", err))?;

        self.tx = estimated_tx;

        Ok(self)
    }

    async fn estimate_script_if_possible(&mut self) -> anyhow::Result<()> {
        if let Some(script) = self.tx.as_script_mut() {
            *script.script_gas_limit_mut() = 0;

            if script.script().is_empty() {
                return Ok(());
            }

            let gas_costs = self.arguments.consensus_parameters.gas_costs();
            let fee_params = self.arguments.consensus_parameters.fee_params();

            let gas_used_by_tx = script.max_gas(gas_costs, fee_params);
            let max_tx_gas = self
                .arguments
                .consensus_parameters
                .tx_params()
                .max_gas_per_tx();

            let max_gas_limit = max_tx_gas.saturating_sub(gas_used_by_tx);

            *script.script_gas_limit_mut() = max_gas_limit;

            let mut status;

            loop {
                status = self.arguments.dry_run(script.clone()).await?;

                let mut contract_not_in_inputs = None;

                match &status.result {
                    TransactionExecutionResult::Success { .. } => break,
                    TransactionExecutionResult::Failed { receipts, .. } => {
                        for receipt in receipts.iter().rev() {
                            if let Receipt::Panic { reason, id, .. } = receipt {
                                if reason.reason() == &PanicReason::ContractNotInInputs {
                                    contract_not_in_inputs = Some(*id);
                                    break;
                                }
                            }
                        }
                    }
                }

                // TODO: Use https://github.com/FuelLabs/fuel-vm/pull/915
                if let Some(contract_id) = contract_not_in_inputs {
                    let inptus = script.inputs_mut();
                    let contract_idx = u16::try_from(inptus.len()).unwrap_or(u16::MAX);

                    inptus.push(Input::contract(
                        Default::default(),
                        Default::default(),
                        Default::default(),
                        Default::default(),
                        contract_id,
                    ));

                    script.outputs_mut().push(Output::contract(
                        contract_idx,
                        Default::default(),
                        Default::default(),
                    ));
                } else {
                    break;
                }
            }

            let new_script_limit =
                status.result.total_gas().saturating_sub(gas_used_by_tx);
            *script.script_gas_limit_mut() = new_script_limit;
        }

        Ok(())
    }

    async fn cover_fee(&mut self) -> anyhow::Result<()> {
        let base_asset_id = *self.arguments.consensus_parameters.base_asset_id();
        let gas_costs = self.arguments.consensus_parameters.gas_costs().clone();
        let fee_params = *self.arguments.consensus_parameters.fee_params();
        let max_gas_per_tx = self
            .arguments
            .consensus_parameters
            .tx_params()
            .max_gas_per_tx();
        let gas_price_factor = fee_params.gas_price_factor();

        let mut total_base_asset = 0u64;

        for input in self.tx.inputs() {
            let Some(amount) = input.amount() else {
                continue;
            };
            let Some(asset_id) = input.asset_id(&base_asset_id) else {
                continue;
            };

            if asset_id == &base_asset_id {
                total_base_asset =
                    total_base_asset.checked_add(amount).ok_or(anyhow::anyhow!(
                        "The total base asset amount used by the transaction is too big"
                    ))?;
            }
        }

        let fee_payer_account = self.fee_payer_account.clone();

        loop {
            let remaining_input_slots = self.remaining_input_slots()?;
            let max_gas = self.tx.max_gas(&gas_costs, &fee_params);
            let max_gas_with_reserve = max_gas.saturating_add(self.arguments.reserve_gas);

            let final_gas = max_gas_with_reserve.min(max_gas_per_tx);
            let final_fee =
                gas_to_fee(final_gas, self.arguments.gas_price, gas_price_factor);
            let final_fee = u64::try_from(final_fee).map_err(|_| {
                anyhow::anyhow!("The final fee is too big to fit into `u64`")
            })?;

            let need_to_cover = final_fee.saturating_add(self.base_asset_reserved);

            if need_to_cover <= total_base_asset {
                break;
            }

            let how_much_to_add = need_to_cover.saturating_sub(total_base_asset);
            let coins = self
                .arguments
                .coins(
                    fee_payer_account.owner(),
                    base_asset_id,
                    how_much_to_add,
                    remaining_input_slots,
                )
                .await?;

            for coin in coins {
                total_base_asset = total_base_asset.checked_add(coin.amount()).ok_or(
                    anyhow::anyhow!(
                        "The total base asset amount \
                        became too big when tried to cover fee"
                    ),
                )?;
                self.add_input_and_witness(&fee_payer_account, coin);
            }
        }

        Ok(())
    }

    fn set_max_fee(&mut self) {
        let gas_costs = self.arguments.consensus_parameters.gas_costs();
        let fee_params = self.arguments.consensus_parameters.fee_params();
        let max_gas_per_tx = self
            .arguments
            .consensus_parameters
            .tx_params()
            .max_gas_per_tx();
        let gas_price_factor = fee_params.gas_price_factor();

        let max_gas = self.tx.max_gas(gas_costs, fee_params);
        let max_gas_with_reserve = max_gas.saturating_add(self.arguments.reserve_gas);

        let final_gas = max_gas_with_reserve.min(max_gas_per_tx);
        let final_fee = gas_to_fee(final_gas, self.arguments.gas_price, gas_price_factor);
        let final_fee = u64::try_from(final_fee).unwrap_or(u64::MAX);

        self.tx.set_max_fee_limit(final_fee);
    }
}

fn gas_to_fee(gas: Word, gas_price: Word, factor: Word) -> u128 {
    let total_price = (gas as u128)
        .checked_mul(gas_price as u128)
        .expect("Impossible to overflow because multiplication of two `u64` <= `u128`");
    total_price.div_ceil(factor as u128)
}
