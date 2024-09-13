use fuel_core_types::{
    blockchain::{
        consensus,
        header::ConsensusParametersVersion,
    },
    fuel_tx::{
        field::UpgradePurpose as _,
        ConsensusParameters,
        Transaction,
        UpgradePurpose,
    },
    fuel_types::BlockHeight,
    fuel_vm::{
        checked_transaction::{
            CheckPredicateParams,
            CheckPredicates,
            Checked,
            CheckedTransaction,
            Checks,
            IntoChecked,
        },
        interpreter::Memory,
    },
    services::txpool::PoolTransaction,
};

use crate::{
    error::Error,
    ports::{
        GasPriceProvider,
        WasmChecker,
    },
    GasPrice,
};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct UnverifiedTx(Transaction);

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct BasicVerifiedTx(Checked<Transaction>);

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct InputDependenciesVerifiedTx(Checked<Transaction>);

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct InputComputationVerifiedTx(Checked<Transaction>);

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct FullyVerifiedTx(Checked<Transaction>);

pub enum TransactionVerifState {
    Unverified(UnverifiedTx),
    BasicVerified(BasicVerifiedTx),
    InputDependenciesVerified(InputDependenciesVerifiedTx),
    InputComputationVerified(InputComputationVerifiedTx),
    FullyVerified(FullyVerifiedTx),
}

impl UnverifiedTx {
    pub async fn perform_basic_verifications(
        self,
        current_height: BlockHeight,
        consensus_params: &ConsensusParameters,
        gas_price_provider: &impl GasPriceProvider,
    ) -> Result<BasicVerifiedTx, Error> {
        if self.0.is_mint() {
            return Err(Error::NotSupportedTransactionType);
        }
        let tx = self
            .0
            .into_checked_basic(current_height, consensus_params)?;
        let gas_price = gas_price_provider.next_gas_price().await?;
        let tx = verify_tx_min_gas_price(tx, consensus_params, gas_price)?;
        Ok(BasicVerifiedTx(tx))
    }
}

impl BasicVerifiedTx {
    pub fn perform_inputs_verifications(
        self,
    ) -> Result<InputDependenciesVerifiedTx, Error> {
        // TODO: Use the pool
        Ok(InputDependenciesVerifiedTx(self.0))
    }
}

impl InputDependenciesVerifiedTx {
    pub async fn perform_input_computation_verifications<M>(
        self,
        consensus_params: &ConsensusParameters,
        wasm_checker: &impl WasmChecker,
        memory: M,
    ) -> Result<InputComputationVerifiedTx, Error>
    where
        M: Memory + Send + Sync + 'static,
    {
        if let Transaction::Upgrade(upgrade) = self.0.transaction() {
            if let UpgradePurpose::StateTransition { root } = upgrade.upgrade_purpose() {
                wasm_checker
                    .validate_uploaded_wasm(root)
                    .map_err(|err| Error::WasmValidity(err))?;
            }
        }

        let parameters = CheckPredicateParams::from(consensus_params);
        let tx =
            tokio_rayon::spawn_fifo(move || self.0.check_predicates(&parameters, memory))
                .await?;

        Ok(InputComputationVerifiedTx(tx))
    }
}

impl InputComputationVerifiedTx {
    pub fn perform_final_verifications(
        self,
        consensus_params: &ConsensusParameters,
    ) -> Result<FullyVerifiedTx, Error> {
        let tx = self.0.check_signatures(&consensus_params.chain_id())?;
        debug_assert!(tx.checks().contains(Checks::all()));
        Ok(FullyVerifiedTx(tx))
    }
}

impl FullyVerifiedTx {
    pub fn into_pool_transaction(
        self,
        version: ConsensusParametersVersion,
    ) -> Result<PoolTransaction, Error> {
        let tx: CheckedTransaction = self.0.into();

        match tx {
            CheckedTransaction::Script(tx) => Ok(PoolTransaction::Script(tx, version)),
            CheckedTransaction::Create(tx) => Ok(PoolTransaction::Create(tx, version)),
            CheckedTransaction::Mint(_) => Err(Error::MintIsDisallowed),
            CheckedTransaction::Upgrade(tx) => Ok(PoolTransaction::Upgrade(tx, version)),
            CheckedTransaction::Upload(tx) => Ok(PoolTransaction::Upload(tx, version)),
            CheckedTransaction::Blob(tx) => Ok(PoolTransaction::Blob(tx, version)),
        }
    }
}

pub async fn perform_all_verifications<M>(
    tx: Transaction,
    current_height: BlockHeight,
    consensus_params: &ConsensusParameters,
    consensus_params_version: ConsensusParametersVersion,
    gas_price_provider: &impl GasPriceProvider,
    wasm_checker: &impl WasmChecker,
    memory: M,
) -> Result<PoolTransaction, Error>
where
    M: Memory + Send + Sync + 'static,
{
    let unverified = UnverifiedTx(tx);
    let basically_verified_tx = unverified
        .perform_basic_verifications(current_height, consensus_params, gas_price_provider)
        .await?;
    let inputs_verified_tx = basically_verified_tx.perform_inputs_verifications()?;
    let input_computation_verified_tx = inputs_verified_tx
        .perform_input_computation_verifications(consensus_params, wasm_checker, memory)
        .await?;
    let fully_verified_tx =
        input_computation_verified_tx.perform_final_verifications(consensus_params)?;
    fully_verified_tx.into_pool_transaction(consensus_params_version)
}

fn verify_tx_min_gas_price(
    tx: Checked<Transaction>,
    consensus_params: &ConsensusParameters,
    gas_price: GasPrice,
) -> Result<Checked<Transaction>, Error> {
    let tx: CheckedTransaction = tx.into();
    let gas_costs = consensus_params.gas_costs();
    let fee_parameters = consensus_params.fee_params();
    let read = match tx {
        CheckedTransaction::Script(script) => {
            let ready = script.into_ready(gas_price, gas_costs, fee_parameters)?;
            let (_, checked) = ready.decompose();
            CheckedTransaction::Script(checked)
        }
        CheckedTransaction::Create(create) => {
            let ready = create.into_ready(gas_price, gas_costs, fee_parameters)?;
            let (_, checked) = ready.decompose();
            CheckedTransaction::Create(checked)
        }
        CheckedTransaction::Upgrade(tx) => {
            let ready = tx.into_ready(gas_price, gas_costs, fee_parameters)?;
            let (_, checked) = ready.decompose();
            CheckedTransaction::Upgrade(checked)
        }
        CheckedTransaction::Upload(tx) => {
            let ready = tx.into_ready(gas_price, gas_costs, fee_parameters)?;
            let (_, checked) = ready.decompose();
            CheckedTransaction::Upload(checked)
        }
        CheckedTransaction::Blob(tx) => {
            let ready = tx.into_ready(gas_price, gas_costs, fee_parameters)?;
            let (_, checked) = ready.decompose();
            CheckedTransaction::Blob(checked)
        }
        CheckedTransaction::Mint(_) => return Err(Error::NotSupportedTransactionType),
    };
    Ok(read.into())
}
