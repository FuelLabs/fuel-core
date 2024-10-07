use crate::{
    error::Error,
    ports::{
        ConsensusParametersProvider,
        GasPriceProvider,
        TxPoolPersistentStorage,
        WasmChecker,
    },
    service::{
        memory::MemoryPool,
        Shared,
        TxPool,
    },
    GasPrice,
};
use fuel_core_storage::transactional::AtomicView;
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
use std::sync::Arc;

pub(super) struct Verification<View> {
    pub persistent_storage_provider: Arc<dyn AtomicView<LatestView = View>>,
    pub consensus_parameters_provider: Arc<dyn ConsensusParametersProvider>,
    pub gas_price_provider: Arc<dyn GasPriceProvider>,
    pub wasm_checker: Arc<dyn WasmChecker>,
    pub memory_pool: MemoryPool,
}

impl<V> Clone for Verification<V> {
    fn clone(&self) -> Self {
        Self {
            persistent_storage_provider: self.persistent_storage_provider.clone(),
            consensus_parameters_provider: self.consensus_parameters_provider.clone(),
            gas_price_provider: self.gas_price_provider.clone(),
            wasm_checker: self.wasm_checker.clone(),
            memory_pool: self.memory_pool.clone(),
        }
    }
}

impl<View> Verification<View>
where
    View: TxPoolPersistentStorage,
{
    pub async fn perform_all_verifications(
        &self,
        tx: Transaction,
        pool: &Shared<TxPool>,
        current_height: BlockHeight,
    ) -> Result<PoolTransaction, Error> {
        let unverified = UnverifiedTx(tx);

        let (version, consensus_params) = self
            .consensus_parameters_provider
            .latest_consensus_parameters();

        let basically_verified_tx = unverified
            .perform_basic_verifications(
                current_height,
                &consensus_params,
                self.gas_price_provider.as_ref(),
            )
            .await?;

        let view = self
            .persistent_storage_provider
            .latest_view()
            .map_err(|e| Error::Database(format!("{:?}", e)))?;

        let inputs_verified_tx =
            basically_verified_tx.perform_inputs_verifications(pool, version, &view)?;

        let fully_verified_tx = inputs_verified_tx
            .perform_input_computation_verifications(
                &consensus_params,
                self.wasm_checker.as_ref(),
                self.memory_pool.take_raw(),
                &view,
            )?;

        fully_verified_tx.into_pool_transaction(version)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(super) struct UnverifiedTx(Transaction);

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(super) struct BasicVerifiedTx(CheckedTransaction);

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(super) struct InputDependenciesVerifiedTx(Checked<Transaction>);

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(super) struct FullyVerifiedTx(Checked<Transaction>);

impl UnverifiedTx {
    pub async fn perform_basic_verifications(
        self,
        current_height: BlockHeight,
        consensus_params: &ConsensusParameters,
        gas_price_provider: &dyn GasPriceProvider,
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
    pub fn perform_inputs_verifications<View>(
        self,
        pool: &Shared<TxPool>,
        version: ConsensusParametersVersion,
        view: &View,
    ) -> Result<InputDependenciesVerifiedTx, Error>
    where
        View: TxPoolPersistentStorage,
    {
        let pool_tx = checked_tx_into_pool(self.0, version)?;

        let transaction = pool
            .read()
            .can_insert_transaction(Arc::new(pool_tx), view)?
            .into_transaction();
        // SAFETY: We created the arc just above and it's not shared.
        let transaction =
            Arc::try_unwrap(transaction).expect("We only the owner of the `Arc`; qed");
        let checked_transaction: CheckedTransaction = transaction.into();
        Ok(InputDependenciesVerifiedTx(checked_transaction.into()))
    }
}

impl InputDependenciesVerifiedTx {
    pub fn perform_input_computation_verifications<View>(
        self,
        consensus_params: &ConsensusParameters,
        wasm_checker: &dyn WasmChecker,
        memory: impl Memory,
        view: &View,
    ) -> Result<FullyVerifiedTx, Error>
    where
        View: TxPoolPersistentStorage,
    {
        let tx = self.0.check_signatures(&consensus_params.chain_id())?;

        let parameters = CheckPredicateParams::from(consensus_params);
        let tx = tx.check_predicates(&parameters, memory, view)?;

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
