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
};
use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::{
    blockchain::header::ConsensusParametersVersion,
    fuel_asm::Word,
    fuel_tx::{
        field::{
            MaxFeeLimit,
            UpgradePurpose as _,
        },
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
        interpreter::{
            ExecutableTransaction,
            Memory,
        },
    },
    services::txpool::{
        Metadata,
        PoolTransaction,
    },
};
use std::sync::Arc;

pub struct Verification<View> {
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
        let (version, consensus_params) = self
            .consensus_parameters_provider
            .latest_consensus_parameters();

        let unverified = UnverifiedTx(tx);

        let basically_verified_tx =
            unverified.perform_basic_verifications(current_height, &consensus_params)?;

        let metadata =
            calculate_metadata(&basically_verified_tx.0, &consensus_params, version)?;
        let minimal_gas_price = self.gas_price_provider.next_gas_price().await?;
        let max_gas_price = metadata.max_gas_price();

        if max_gas_price < minimal_gas_price {
            return Err(Error::InsufficientMaxFee {
                max_gas_price_from_fee: max_gas_price,
                minimal_gas_price,
            });
        }

        let view = self
            .persistent_storage_provider
            .latest_view()
            .map_err(|e| Error::Database(format!("{:?}", e)))?;

        let inputs_verified_tx =
            basically_verified_tx.perform_inputs_verifications(pool, &view, metadata)?;

        let fully_verified_tx = inputs_verified_tx
            .perform_input_computation_verifications(
                &consensus_params,
                self.wasm_checker.as_ref(),
                self.memory_pool.take_raw(),
                &view,
            )?;

        fully_verified_tx.into_pool_transaction(metadata)
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
    pub fn perform_basic_verifications(
        self,
        current_height: BlockHeight,
        consensus_params: &ConsensusParameters,
    ) -> Result<BasicVerifiedTx, Error> {
        if self.0.is_mint() {
            return Err(Error::MintIsDisallowed);
        }

        let tx = self
            .0
            .into_checked_basic(current_height, consensus_params)?;

        Ok(BasicVerifiedTx(tx.into()))
    }
}

impl BasicVerifiedTx {
    pub fn perform_inputs_verifications<View>(
        self,
        pool: &Shared<TxPool>,
        view: &View,
        metadata: Metadata,
    ) -> Result<InputDependenciesVerifiedTx, Error>
    where
        View: TxPoolPersistentStorage,
    {
        let pool_tx = checked_tx_into_pool(self.0, metadata)?;

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
        metadata: Metadata,
    ) -> Result<PoolTransaction, Error> {
        checked_tx_into_pool(self.0.into(), metadata)
    }
}

fn calculate_metadata(
    tx: &CheckedTransaction,
    consensus_params: &ConsensusParameters,
    version: ConsensusParametersVersion,
) -> Result<Metadata, Error> {
    let metadata = match tx {
        CheckedTransaction::Script(tx) => metadata_for_tx(tx, consensus_params, version),
        CheckedTransaction::Create(tx) => metadata_for_tx(tx, consensus_params, version),
        CheckedTransaction::Mint(_) => {
            return Err(Error::MintIsDisallowed);
        }
        CheckedTransaction::Upgrade(tx) => metadata_for_tx(tx, consensus_params, version),
        CheckedTransaction::Upload(tx) => metadata_for_tx(tx, consensus_params, version),
        CheckedTransaction::Blob(tx) => metadata_for_tx(tx, consensus_params, version),
    };

    Ok(metadata)
}

fn metadata_for_tx<Tx>(
    tx: &Checked<Tx>,
    consensus_params: &ConsensusParameters,
    version: ConsensusParametersVersion,
) -> Metadata
where
    Tx: ExecutableTransaction,
{
    // `max_fee_limit` is always set for `Checked<Tx>`
    //
    // gas * gas_price / gas_price_factor = fee
    // gas_price = fee * gas_price_factor / gas
    // max_gas_price = max_fee * gas_price_factor / max_gas
    let gas_costs = consensus_params.gas_costs();
    let fee_parameters = consensus_params.fee_params();
    let max_gas = tx.transaction().max_gas(gas_costs, fee_parameters) as u128;
    let gas_price_factor = fee_parameters.gas_price_factor() as u128;

    let max_fee = tx.transaction().max_fee_limit() as u128;

    let max_gas_price = max_fee
        .saturating_mul(gas_price_factor)
        .saturating_div(max_gas.saturating_add(1));
    let metered_bytes_size = tx.transaction().metered_bytes_size();

    Metadata::new(
        version,
        metered_bytes_size,
        Word::try_from(max_gas_price).unwrap_or(Word::MAX),
    )
}

pub fn checked_tx_into_pool(
    tx: CheckedTransaction,
    metadata: Metadata,
) -> Result<PoolTransaction, Error> {
    match tx {
        CheckedTransaction::Script(tx) => Ok(PoolTransaction::Script(tx, metadata)),
        CheckedTransaction::Create(tx) => Ok(PoolTransaction::Create(tx, metadata)),
        CheckedTransaction::Mint(_) => Err(Error::MintIsDisallowed),
        CheckedTransaction::Upgrade(tx) => Ok(PoolTransaction::Upgrade(tx, metadata)),
        CheckedTransaction::Upload(tx) => Ok(PoolTransaction::Upload(tx, metadata)),
        CheckedTransaction::Blob(tx) => Ok(PoolTransaction::Blob(tx, metadata)),
    }
}
