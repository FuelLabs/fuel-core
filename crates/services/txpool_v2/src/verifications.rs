use std::sync::Arc;

use fuel_core_types::{
    blockchain::header::ConsensusParametersVersion,
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
    services::txpool::{
        Metadata,
        PoolTransaction,
    },
};

use crate::{
    error::Error,
    ports::{
        AtomicView,
        GasPriceProvider,
        TxPoolPersistentStorage,
        WasmChecker,
    },
    shared_state::TxPool,
    GasPrice,
};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct UnverifiedTx(Transaction);

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct BasicVerifiedTx(CheckedTransaction);

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct InputDependenciesVerifiedTx(Checked<Transaction>);

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct FullyVerifiedTx(Checked<Transaction>);

impl UnverifiedTx {
    pub async fn perform_basic_verifications(
        self,
        current_height: BlockHeight,
        consensus_params: &ConsensusParameters,
        gas_price_provider: &impl GasPriceProvider,
    ) -> Result<BasicVerifiedTx, Error> {
        if self.0.is_mint() {
            return Err(Error::MintIsDisallowed);
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
    pub fn perform_inputs_verifications<PSView, PSProvider>(
        self,
        pool: TxPool<PSProvider>,
        version: ConsensusParametersVersion,
    ) -> Result<InputDependenciesVerifiedTx, Error>
    where
        PSProvider: AtomicView<LatestView = PSView>,
        PSView: TxPoolPersistentStorage,
    {
        let pool_tx = checked_tx_into_pool(self.0, version)?;

        let transaction = pool
            .read()
            .can_insert_transaction(Arc::new(pool_tx))?
            .into_transaction();
        // SAFETY: We created the arc just above and it's not shared.
        let transaction = Arc::try_unwrap(transaction).unwrap();
        let checked_transaction: CheckedTransaction = transaction.into();
        Ok(InputDependenciesVerifiedTx(checked_transaction.into()))
    }
}

impl InputDependenciesVerifiedTx {
    pub fn perform_input_computation_verifications<M>(
        self,
        consensus_params: &ConsensusParameters,
        wasm_checker: &impl WasmChecker,
        memory: M,
    ) -> Result<FullyVerifiedTx, Error>
    where
        M: Memory + Send + Sync + 'static,
    {
        let tx = self.0.check_signatures(&consensus_params.chain_id())?;

        let parameters = CheckPredicateParams::from(consensus_params);
        let tx = tx.check_predicates(&parameters, memory)?;

        if let Transaction::Upgrade(upgrade) = tx.transaction() {
            if let UpgradePurpose::StateTransition { root } = upgrade.upgrade_purpose() {
                wasm_checker
                    .validate_uploaded_wasm(root)
                    .map_err(Error::WasmValidity)?;
            }
        }

        debug_assert!(tx.checks().contains(Checks::all()));

        Ok(FullyVerifiedTx(tx))
    }
}

impl FullyVerifiedTx {
    pub fn into_pool_transaction(
        self,
        version: ConsensusParametersVersion,
    ) -> Result<PoolTransaction, Error> {
        checked_tx_into_pool(self.0.into(), version)
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn perform_all_verifications<M, PSView, PSProvider>(
    tx: Transaction,
    pool: TxPool<PSProvider>,
    current_height: BlockHeight,
    consensus_params: &ConsensusParameters,
    consensus_params_version: ConsensusParametersVersion,
    gas_price_provider: &impl GasPriceProvider,
    wasm_checker: &impl WasmChecker,
    memory: M,
) -> Result<PoolTransaction, Error>
where
    M: Memory + Send + Sync + 'static,
    PSProvider: AtomicView<LatestView = PSView>,
    PSView: TxPoolPersistentStorage,
{
    let unverified = UnverifiedTx(tx);
    let basically_verified_tx = unverified
        .perform_basic_verifications(current_height, consensus_params, gas_price_provider)
        .await?;
    let inputs_verified_tx = basically_verified_tx
        .perform_inputs_verifications(pool, consensus_params_version)?;
    let fully_verified_tx = inputs_verified_tx.perform_input_computation_verifications(
        consensus_params,
        wasm_checker,
        memory,
    )?;
    fully_verified_tx.into_pool_transaction(consensus_params_version)
}

fn verify_tx_min_gas_price(
    tx: Checked<Transaction>,
    consensus_params: &ConsensusParameters,
    gas_price: GasPrice,
) -> Result<CheckedTransaction, Error> {
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
        CheckedTransaction::Mint(_) => return Err(Error::MintIsDisallowed),
    };
    Ok(read)
}

pub fn checked_tx_into_pool(
    tx: CheckedTransaction,
    version: ConsensusParametersVersion,
) -> Result<PoolTransaction, Error> {
    let metadata = Metadata::new(version);
    match tx {
        CheckedTransaction::Script(tx) => Ok(PoolTransaction::Script(tx, metadata)),
        CheckedTransaction::Create(tx) => Ok(PoolTransaction::Create(tx, metadata)),
        CheckedTransaction::Mint(_) => Err(Error::MintIsDisallowed),
        CheckedTransaction::Upgrade(tx) => Ok(PoolTransaction::Upgrade(tx, metadata)),
        CheckedTransaction::Upload(tx) => Ok(PoolTransaction::Upload(tx, metadata)),
        CheckedTransaction::Blob(tx) => Ok(PoolTransaction::Blob(tx, metadata)),
    }
}
