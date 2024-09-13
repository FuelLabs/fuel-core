use std::sync::Arc;

use fuel_core_types::{
    fuel_tx::{
        ConsensusParameters,
        Transaction,
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
};

use crate::{
    error::Error,
    ports::{
        GasPriceProvider,
        MemoryPool,
    },
    GasPrice,
};

pub async fn check_transactions<Provider, MP>(
    txs: Vec<Transaction>,
    current_height: BlockHeight,
    utxp_validation: bool,
    consensus_params: &ConsensusParameters,
    gas_price_provider: &Provider,
    memory_pool: Arc<MP>,
) -> Vec<Result<Checked<Transaction>, Error>>
where
    Provider: GasPriceProvider,
    MP: MemoryPool,
{
    let mut checked_txs = Vec::with_capacity(txs.len());

    for tx in txs.into_iter() {
        checked_txs.push(
            check_single_tx(
                tx,
                current_height,
                utxp_validation,
                consensus_params,
                gas_price_provider,
                memory_pool.get_memory().await,
            )
            .await,
        );
    }

    checked_txs
}

pub async fn check_single_tx<GasPrice, M>(
    tx: Transaction,
    current_height: BlockHeight,
    utxo_validation: bool,
    consensus_params: &ConsensusParameters,
    gas_price_provider: &GasPrice,
    memory: M,
) -> Result<Checked<Transaction>, Error>
where
    GasPrice: GasPriceProvider,
    M: Memory + Send + Sync + 'static,
{
    if tx.is_mint() {
        return Err(Error::NotSupportedTransactionType);
    }

    let tx: Checked<Transaction> = if utxo_validation {
        let tx = tx
            .into_checked_basic(current_height, consensus_params)?
            .check_signatures(&consensus_params.chain_id())?;

        let parameters = CheckPredicateParams::from(consensus_params);
        let tx =
            tokio_rayon::spawn_fifo(move || tx.check_predicates(&parameters, memory))
                .await?;

        debug_assert!(tx.checks().contains(Checks::all()));

        tx
    } else {
        tx.into_checked_basic(current_height, consensus_params)?
    };

    let gas_price = gas_price_provider.next_gas_price().await?;

    let tx = verify_tx_min_gas_price(tx, consensus_params, gas_price)?;

    Ok(tx)
}

fn verify_tx_min_gas_price(
    tx: Checked<Transaction>,
    consensus_params: &ConsensusParameters,
    gas_price: GasPrice,
) -> Result<Checked<Transaction>, Error> {
    let tx: CheckedTransaction = tx.into();
    let gas_costs = consensus_params.gas_costs();
    let fee_parameters = consensus_params.fee_params();
    let ready_tx = match tx {
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
    Ok(ready_tx.into())
}
