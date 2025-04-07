use crate::{
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
        op,
        PanicReason,
        Word,
    },
    fuel_crypto::Signature,
    fuel_tx::{
        field::{
            Inputs,
            MaxFeeLimit,
            Outputs,
            Policies,
            Script as ScriptField,
            ScriptGasLimit,
            Tip,
            WitnessLimit,
        },
        input::{
            coin::CoinSigned,
            message::{
                MessageCoinSigned,
                MessageDataSigned,
            },
        },
        policies::PolicyType,
        Address,
        AssetId,
        Cacheable,
        Chargeable,
        ConsensusParameters,
        Input,
        Output,
        Receipt,
        Script,
        Transaction,
    },
    fuel_types::{
        canonical::Serialize,
        ContractId,
    },
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

pub struct AssembleArguments<'a> {
    pub fee_index: u16,
    pub required_balances: Vec<RequiredBalance>,
    pub exclude: Exclude,
    pub estimate_predicates: bool,
    pub reserve_gas: u64,
    pub consensus_parameters: Arc<ConsensusParameters>,
    pub gas_price: u64,
    pub dry_run_limit: usize,
    pub estimate_predicates_limit: usize,
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
        allow_partial: bool,
    ) -> anyhow::Result<Vec<CoinType>> {
        if amount == 0 {
            return Ok(Vec::new());
        }

        let query_per_asset = SpendQueryElementInput {
            asset_id: asset_id.into(),
            amount: (amount as u128).into(),
            max: None,
            allow_partial: Some(allow_partial),
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
            .next();

        let result =
            result.ok_or_else(|| anyhow::anyhow!("No result for the coins to spend"))?;

        Ok(result)
    }

    /// Dry run the transaction to estimate the script gas limit.
    /// It uses zero gas price to not depend on the coins to cover the fee.
    async fn dry_run(
        &self,
        script: Script,
    ) -> anyhow::Result<(Transaction, TransactionExecutionStatus)> {
        self.block_producer
            .dry_run_txs(vec![script.into()], None, None, Some(false), Some(0), false)
            .await?
            .transactions
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No result for the dry run"))
    }
}

pub struct AssembleTx<'a, Tx> {
    tx: Tx,
    arguments: AssembleArguments<'a>,
    signature_witness_indexes: HashMap<Address, u16>,
    change_output_policies: HashMap<AssetId, ChangePolicy>,
    set_change_outputs: HashSet<AssetId>,
    set_contracts: HashSet<ContractId>,
    // The amount of the base asset that is reserved for the application logic
    base_asset_reserved: u64,
    has_predicates: bool,
    index_of_first_fake_variable_output: Option<u16>,
    original_max_fee: u64,
    original_witness_limit: u64,
    fee_payer_account: Account,
    estimated_predicates_count: usize,
    dry_run_count: usize,
}

impl<'a, Tx> AssembleTx<'a, Tx>
where
    Tx: ExecutableTransaction + Cacheable + Send + 'static,
{
    pub fn new(tx: Tx, mut arguments: AssembleArguments<'a>) -> anyhow::Result<Self> {
        let max_inputs = arguments.consensus_parameters.tx_params().max_inputs();
        let max_outputs = arguments.consensus_parameters.tx_params().max_outputs();

        if tx.inputs().len() > max_inputs as usize {
            return Err(anyhow::anyhow!(
                "The transaction has more inputs than allowed by the consensus"
            ));
        }

        if tx.outputs().len() > max_outputs as usize {
            return Err(anyhow::anyhow!(
                "The transaction has more outputs than allowed by the consensus"
            ));
        }

        if arguments.fee_index as usize >= arguments.required_balances.len() {
            return Err(anyhow::anyhow!("The fee address index is out of bounds"));
        }

        if has_duplicates(&arguments.required_balances, |balance| {
            (balance.asset_id, balance.account.owner())
        }) {
            return Err(anyhow::anyhow!(
                "required balances contain duplicate (asset, account) pair"
            ));
        }

        let base_asset_id = *arguments.consensus_parameters.base_asset_id();
        let mut signature_witness_indexes = HashMap::<Address, u16>::new();

        // Exclude inputs that already are used by the transaction
        let mut has_predicates = false;
        let inputs = tx.inputs();
        let mut unique_used_asset = HashSet::new();
        let mut set_contracts = HashSet::new();
        for input in inputs {
            if let Some(utxo_id) = input.utxo_id() {
                arguments.exclude.exclude(CoinId::Utxo(*utxo_id));
            }

            if let Some(nonce) = input.nonce() {
                arguments.exclude.exclude(CoinId::Message(*nonce));
            }

            if let Some(asset_id) = input.asset_id(&base_asset_id) {
                unique_used_asset.insert(*asset_id);
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
                Input::Contract(c) => {
                    set_contracts.insert(c.contract_id);
                }

                Input::CoinPredicate(_)
                | Input::MessageCoinPredicate(_)
                | Input::MessageDataPredicate(_) => {
                    has_predicates = true;
                }
            }
        }

        let mut change_output_policies = HashMap::<AssetId, ChangePolicy>::new();
        let mut set_change_outputs = HashSet::<AssetId>::new();

        for output in tx.outputs() {
            if let Output::Change { to, asset_id, .. } = output {
                change_output_policies.insert(*asset_id, ChangePolicy::Change(*to));
                set_change_outputs.insert(*asset_id);
            }
        }

        let mut base_asset_reserved: Option<u64> = None;

        let fee_payer_account = arguments
            .required_balances
            .get(arguments.fee_index as usize)
            .ok_or_else(|| anyhow::anyhow!("fee index out of bounds"))?
            .account
            .clone();

        let mut requested_asset = HashSet::new();
        for required_balance in &arguments.required_balances {
            let asset_id = required_balance.asset_id;
            requested_asset.insert(asset_id);

            if asset_id == base_asset_id
                && fee_payer_account.owner() == required_balance.account.owner()
            {
                base_asset_reserved = Some(required_balance.amount);
            }
        }

        for input_asset_id in &unique_used_asset {
            // If the user didn't request the asset, we add it to the required balances
            // with minimal amount `0` and `ChangePolicy::Change` policy.
            if !requested_asset.contains(input_asset_id) {
                let recipient = fee_payer_account.owner();

                arguments.required_balances.push(RequiredBalance {
                    account: fee_payer_account.clone(),
                    asset_id: *input_asset_id,
                    amount: 0,
                    change_policy: ChangePolicy::Change(recipient),
                });
            }
        }

        if base_asset_reserved.is_none() {
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
                        ));
                    }
                }
                Entry::Vacant(vacant) => {
                    vacant.insert(required_balance.change_policy);
                }
            }
        }

        // Removed required balances with zero amount and if they are not used in inputs
        let required_balances = core::mem::take(&mut arguments.required_balances);
        arguments.required_balances = required_balances
            .into_iter()
            .filter_map(|r| {
                if r.amount != 0 || unique_used_asset.contains(&r.asset_id) {
                    Some(r)
                } else {
                    None
                }
            })
            .collect();

        let original_max_fee = tx.max_fee_limit();
        let original_witness_limit = tx.witness_limit();

        Ok(Self {
            tx,
            arguments,
            signature_witness_indexes,
            change_output_policies,
            set_change_outputs,
            set_contracts,
            base_asset_reserved: base_asset_reserved.expect("Set above; qed"),
            has_predicates,
            index_of_first_fake_variable_output: None,
            original_max_fee,
            original_witness_limit,
            fee_payer_account,
            estimated_predicates_count: 0,
            dry_run_count: 0,
        })
    }

    pub fn add_missing_witnesses(&mut self) {
        let witnesses = self.tx.witnesses_mut();
        for (_, witness_index) in self.signature_witness_indexes.iter() {
            let witness_index = *witness_index as usize;
            if witness_index >= witnesses.len() {
                witnesses.resize(witness_index.saturating_add(1), Vec::new().into());
            }

            let witness = witnesses[witness_index].as_vec_mut();
            if witness.len() < Signature::LEN {
                witness.resize(Signature::LEN, 0);
            }
        }
    }

    pub async fn assemble(mut self) -> anyhow::Result<Tx> {
        self.add_missing_witnesses();
        self.add_inputs_and_witnesses_and_changes().await?;

        if let Some(script) = self.tx.as_script_mut() {
            *script.script_gas_limit_mut() = 0;
        }

        self = self.cover_fee().await?;

        self.adjust_witness_limit();

        self.fill_with_variable_outputs()?;

        // The `cover_fee` already can estimate predicates inside,
        // we don't need to duplicate the work, if it was already done.
        if self.estimated_predicates_count == 0 {
            self = self.estimate_predicates().await?;
        }

        self.estimate_script_if_possible().await?;

        self.remove_unused_variable_outputs();

        self = self.cover_fee().await?;

        self.adjust_witness_limit();

        Ok(self.tx)
    }

    fn remaining_input_slots(&self) -> anyhow::Result<u16> {
        let max_input = self.arguments.consensus_parameters.tx_params().max_inputs();
        let used_inputs = u16::try_from(self.tx.inputs().len()).unwrap_or(u16::MAX);

        if used_inputs > max_input {
            return Err(anyhow::anyhow!(
                "Filling required balances occupies a number \
                    of inputs more than can fit into the transaction"
            ));
        }

        Ok(max_input.saturating_sub(used_inputs))
    }

    async fn add_inputs_and_witnesses_and_changes(&mut self) -> anyhow::Result<()> {
        let required_balance = core::mem::take(&mut self.arguments.required_balances);

        for required_balance in required_balance {
            let remaining_input_slots = self.remaining_input_slots()?;

            let asset_id = required_balance.asset_id;
            let amount = required_balance.amount;
            let owner = required_balance.account.owner();

            self.satisfy_change_policy(asset_id)?;

            let selected_coins = self
                .arguments
                .coins(owner, asset_id, amount, remaining_input_slots, false)
                .await?;

            for coin in selected_coins
                .into_iter()
                .take(remaining_input_slots as usize)
            {
                self.add_input_and_witness_and_change(&required_balance.account, coin)?;
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

    fn add_input_and_witness_and_change(
        &mut self,
        account: &Account,
        coin: CoinType,
    ) -> anyhow::Result<()> {
        let base_asset_id = *self.arguments.consensus_parameters.base_asset_id();

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
                self.has_predicates = true;
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

        if let Some(asset_id) = input.asset_id(&base_asset_id) {
            self.satisfy_change_policy(*asset_id)?;
        }

        self.tx.inputs_mut().push(input);

        let max_inputs = self.arguments.consensus_parameters.tx_params().max_inputs();

        if self.tx.inputs().len() > max_inputs as usize {
            return Err(anyhow::anyhow!(
                "Unable to add more inputs \
                because reached the maximum allowed inputs limit"
            ));
        }

        Ok(())
    }

    fn satisfy_change_policy(&mut self, asset_id: AssetId) -> anyhow::Result<()> {
        if self.set_change_outputs.insert(asset_id) {
            let change_policy =
                if let Some(policy) = self.change_output_policies.get(&asset_id) {
                    *policy
                } else {
                    ChangePolicy::Change(self.fee_payer_account.owner())
                };

            match change_policy {
                ChangePolicy::Change(change_receiver) => {
                    self.tx.outputs_mut().push(Output::change(
                        change_receiver,
                        0,
                        asset_id,
                    ));

                    let max_outputs = self
                        .arguments
                        .consensus_parameters
                        .tx_params()
                        .max_outputs();

                    if self.tx.outputs().len() > max_outputs as usize {
                        return Err(anyhow::anyhow!(
                            "Unable to add more `Change` outputs \
                            because reached the maximum allowed outputs limit"
                        ));
                    }
                }
                ChangePolicy::Destroy => {
                    // Do nothing for now, since `fuel-tx` crate doesn't have
                    // `Destroy` output yet.
                    // https://github.com/FuelLabs/fuel-specs/issues/621
                }
            }
        }

        Ok(())
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
    fn fill_with_variable_outputs(&mut self) -> anyhow::Result<()> {
        if !self.is_runnable_script() {
            return Ok(())
        }

        let max_outputs = self
            .arguments
            .consensus_parameters
            .tx_params()
            .max_outputs();

        let outputs = u16::try_from(self.tx.outputs().len()).unwrap_or(u16::MAX);

        self.tx.outputs_mut().resize(
            max_outputs as usize,
            Output::variable(Default::default(), Default::default(), Default::default()),
        );

        self.index_of_first_fake_variable_output = Some(outputs);

        Ok(())
    }

    fn remove_unused_variable_outputs(&mut self) {
        if !self.is_runnable_script() {
            return
        }

        if self.index_of_first_fake_variable_output.is_none() {
            return
        }

        let index_of_first_fake_variable_output = self
            .index_of_first_fake_variable_output
            .take()
            .expect("Checked above; qed");

        while let Some(output) = self.tx.outputs().last() {
            if self.tx.outputs().len() <= index_of_first_fake_variable_output as usize {
                break
            }

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
        // If the user sets the `WitnessLimit` policy, we are only allowed to increase
        // it in the case if the transaction got more witnesses when we inserted new inputs.
        let mut witness_size = self.tx.witnesses().size_dynamic() as u64;
        witness_size = witness_size.max(self.original_witness_limit);
        self.tx.set_witness_limit(witness_size);
    }

    async fn estimate_predicates(mut self) -> anyhow::Result<Self> {
        if !self.arguments.estimate_predicates {
            return Ok(self)
        }

        if !self.has_predicates {
            return Ok(self)
        }

        if self.estimated_predicates_count >= self.arguments.estimate_predicates_limit {
            return Err(anyhow::anyhow!(
                "The transaction estimation requires running \
                of predicate more than {} times",
                self.arguments.estimate_predicates_limit
            ));
        }

        let memory = self.arguments.shared_memory_pool.get_memory().await;
        let chain_id = self.arguments.consensus_parameters.chain_id();
        self.tx
            .precompute(&chain_id)
            .map_err(|err| anyhow::anyhow!("{:?}", err))?;

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

        self.estimated_predicates_count = self
            .estimated_predicates_count
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("estimated predicates count overflow"))?;

        Ok(self)
    }

    async fn estimate_script_if_possible(&mut self) -> anyhow::Result<()> {
        if !self.is_runnable_script() {
            return Ok(())
        }

        let Some(script_ref) = self.tx.as_script_mut() else {
            unreachable!("The transaction is a script, checked above; qed");
        };

        // Trick to avoid cloning `Script`
        let dummy_script = Transaction::script(
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        let mut script = core::mem::replace(script_ref, dummy_script);

        let dry_run_gas_price = 0;
        set_max_fee(
            &mut script,
            &self.arguments.consensus_parameters,
            self.arguments.reserve_gas,
            dry_run_gas_price,
            self.original_max_fee,
        );

        let max_tx_gas = self
            .arguments
            .consensus_parameters
            .tx_params()
            .max_gas_per_tx();

        let has_spendable_input = script.inputs().iter().any(|input| match input {
            Input::CoinSigned(_)
            | Input::CoinPredicate(_)
            | Input::MessageCoinSigned(_)
            | Input::MessageCoinPredicate(_) => true,
            Input::MessageDataSigned(_)
            | Input::MessageDataPredicate(_)
            | Input::Contract(_) => false,
        });

        // If `max_fee` policy is not set, set it because it affects the maximum gas
        // used by transaction.
        if script.policies().get(PolicyType::MaxFee).is_none() {
            script.set_max_fee_limit(0);
        }

        let (mut script, status) = self
            .populate_missing_contract_inputs(script, has_spendable_input, max_tx_gas)
            .await?;

        let mut total_gas_used = 0u64;

        for receipt in status.result.receipts() {
            if let Receipt::ScriptResult { gas_used, .. } = receipt {
                total_gas_used = total_gas_used.saturating_add(*gas_used);
            }
        }

        *script.script_gas_limit_mut() = total_gas_used;

        let Some(script_ref) = self.tx.as_script_mut() else {
            unreachable!("The transaction is a script, checked above; qed");
        };
        *script_ref = script;

        Ok(())
    }

    async fn populate_missing_contract_inputs(
        &mut self,
        mut script: Script,
        has_spendable_input: bool,
        max_tx_gas: u64,
    ) -> Result<(Script, TransactionExecutionStatus), anyhow::Error> {
        let mut status: TransactionExecutionStatus;

        let gas_costs = self.arguments.consensus_parameters.gas_costs();
        let fee_params = self.arguments.consensus_parameters.fee_params();

        loop {
            if self.dry_run_count > self.arguments.dry_run_limit {
                return Err(anyhow::anyhow!(
                    "The transaction script estimation \
                    requires running of script more than {} times",
                    self.arguments.dry_run_limit
                ));
            }

            if !has_spendable_input {
                script.inputs_mut().push(fake_input());
            }

            // We want to calculate `max_gas` for the script, but without script limit
            *script.script_gas_limit_mut() = 0;

            let gas_used_by_tx = script.max_gas(gas_costs, fee_params);

            let max_gas_limit = max_tx_gas.saturating_sub(gas_used_by_tx);

            *script.script_gas_limit_mut() = max_gas_limit;

            let (updated_tx, new_status) = self.arguments.dry_run(script).await?;

            self.dry_run_count = self
                .dry_run_count
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("dry run count overflow"))?;

            let Transaction::Script(updated_script) = updated_tx else {
                return Err(anyhow::anyhow!(
                    "During script gas limit estimation, \
                        dry-run returned incorrect transaction"
                ));
            };

            script = updated_script;
            status = new_status;

            if !has_spendable_input {
                script.inputs_mut().pop();
            }

            let mut contracts_not_in_inputs = Vec::new();

            match &status.result {
                TransactionExecutionResult::Success { .. } => break,
                TransactionExecutionResult::Failed { receipts, .. } => {
                    for receipt in receipts.iter() {
                        if let Receipt::Panic {
                            reason,
                            contract_id,
                            ..
                        } = receipt
                        {
                            if reason.reason() == &PanicReason::ContractNotInInputs {
                                let contract_id = contract_id.ok_or_else(|| {
                                    anyhow::anyhow!("missing contract id")
                                })?;
                                contracts_not_in_inputs.push(contract_id);
                            }
                        }
                    }
                }
            }

            if contracts_not_in_inputs.is_empty() {
                break;
            }

            for contract_id in contracts_not_in_inputs {
                if !self.set_contracts.insert(contract_id) {
                    continue
                }

                let inptus = script.inputs_mut();

                let contract_idx = u16::try_from(inptus.len()).unwrap_or(u16::MAX);

                inptus.push(Input::contract(
                    Default::default(),
                    Default::default(),
                    Default::default(),
                    Default::default(),
                    contract_id,
                ));

                let slot = self
                    .index_of_first_fake_variable_output
                    .and_then(|index| script.outputs_mut().get_mut(index as usize));

                if let Some(slot) = slot {
                    *slot = Output::contract(
                        contract_idx,
                        Default::default(),
                        Default::default(),
                    );
                    self.index_of_first_fake_variable_output = self
                        .index_of_first_fake_variable_output
                        .and_then(|index| index.checked_add(1));
                } else {
                    return Err(anyhow::anyhow!(
                        "Run out of slots for the contract outputs"
                    ));
                }
            }
        }

        Ok((script, status))
    }

    async fn cover_fee(mut self) -> anyhow::Result<Self> {
        let base_asset_id = *self.arguments.consensus_parameters.base_asset_id();
        let gas_costs = self.arguments.consensus_parameters.gas_costs().clone();
        let fee_params = *self.arguments.consensus_parameters.fee_params();
        let max_gas_per_tx = self
            .arguments
            .consensus_parameters
            .tx_params()
            .max_gas_per_tx();
        let gas_price_factor = fee_params.gas_price_factor();
        let fee_payer_account = self.fee_payer_account.clone();

        let mut total_base_asset = 0u64;

        for input in self.tx.inputs() {
            let Some(amount) = input.amount() else {
                continue;
            };
            let Some(asset_id) = input.asset_id(&base_asset_id) else {
                continue;
            };
            let Some(owner) = input.input_owner() else {
                continue;
            };

            if asset_id == &base_asset_id && &fee_payer_account.owner() == owner {
                total_base_asset =
                    total_base_asset.checked_add(amount).ok_or_else(|| {
                        anyhow::anyhow!(
                        "The total base asset amount used by the transaction is too big"
                    )
                    })?;
            }
        }

        loop {
            let max_gas = self.tx.max_gas(&gas_costs, &fee_params);
            let max_gas_with_reserve = max_gas.saturating_add(self.arguments.reserve_gas);

            let final_gas = max_gas_with_reserve.min(max_gas_per_tx);
            let final_fee =
                gas_to_fee(final_gas, self.arguments.gas_price, gas_price_factor);
            let final_fee = u64::try_from(final_fee)
                .map_err(|_| {
                    anyhow::anyhow!("The final fee is too big to fit into `u64`")
                })?
                .saturating_add(self.tx.tip());

            let need_to_cover = final_fee.saturating_add(self.base_asset_reserved);

            if need_to_cover <= total_base_asset {
                break;
            }

            let remaining_input_slots = self.remaining_input_slots()?;

            let how_much_to_add = need_to_cover.saturating_sub(total_base_asset);
            let coins = self
                .arguments
                .coins(
                    fee_payer_account.owner(),
                    base_asset_id,
                    how_much_to_add,
                    remaining_input_slots,
                    true,
                )
                .await?;

            if coins.is_empty() {
                return Err(anyhow::anyhow!(
                    "Unable to find any coins to pay for the fee"
                ));
            }

            for coin in coins.into_iter().take(remaining_input_slots as usize) {
                total_base_asset = total_base_asset.checked_add(coin.amount()).ok_or(
                    anyhow::anyhow!(
                        "The total base asset amount \
                        became too big when tried to cover fee"
                    ),
                )?;
                self.add_input_and_witness_and_change(&fee_payer_account, coin)?;
            }

            // In the case when predicates iterates over the inputs,
            // it increases its used gas. So we need to re-estimate predicates.
            self = self.estimate_predicates().await?;
        }

        set_max_fee(
            &mut self.tx,
            &self.arguments.consensus_parameters,
            self.arguments.reserve_gas,
            self.arguments.gas_price,
            self.original_max_fee,
        );

        Ok(self)
    }
}

fn has_duplicates<T, F, K>(items: &[T], extractor: F) -> bool
where
    F: Fn(&T) -> K,
    K: std::hash::Hash + std::cmp::Eq,
{
    let mut duplicates = HashSet::with_capacity(items.len());
    for item in items {
        let key = extractor(item);
        if !duplicates.insert(key) {
            return true
        }
    }

    false
}

fn set_max_fee<Tx>(
    tx: &mut Tx,
    consensus_parameters: &ConsensusParameters,
    reserve_gas: u64,
    gas_price: u64,
    original_max_fee: u64,
) where
    Tx: ExecutableTransaction,
{
    let gas_costs = consensus_parameters.gas_costs();
    let fee_params = consensus_parameters.fee_params();
    let max_gas_per_tx = consensus_parameters.tx_params().max_gas_per_tx();
    let gas_price_factor = fee_params.gas_price_factor();

    let max_gas = tx.max_gas(gas_costs, fee_params);
    let max_gas_with_reserve = max_gas.saturating_add(reserve_gas);

    let final_gas = max_gas_with_reserve.min(max_gas_per_tx);
    let final_fee = gas_to_fee(final_gas, gas_price, gas_price_factor);
    let mut final_fee = u64::try_from(final_fee).unwrap_or(u64::MAX);
    final_fee = final_fee.saturating_add(tx.tip());

    // If the user sets the `MaxFee` policy, we are only allowed to increase
    // it in the case if the transaction requires more fee to cover it.
    final_fee = final_fee.max(original_max_fee);
    tx.set_max_fee_limit(final_fee);
}

fn gas_to_fee(gas: Word, gas_price: Word, factor: Word) -> u128 {
    let total_price = (gas as u128)
        .checked_mul(gas_price as u128)
        .expect("Impossible to overflow because multiplication of two `u64` <= `u128`");
    total_price.div_ceil(factor as u128)
}

fn fake_input() -> Input {
    Input::coin_predicate(
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        [op::ret(1)].into_iter().collect(),
        Default::default(),
    )
}
