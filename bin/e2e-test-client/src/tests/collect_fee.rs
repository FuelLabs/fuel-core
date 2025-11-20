use crate::test_context::TestContext;
use fuel_core_types::fuel_tx::Receipt;
use libtest_mimic::Failed;

// Alice collects tokens from coinbase contract.
pub async fn collect_fee(ctx: &TestContext) -> Result<(), Failed> {
    let tx = ctx
        .alice
        .collect_fee_tx(
            ctx.config.coinbase_contract_id,
            *ctx.alice.consensus_params.base_asset_id(),
        )
        .await?;
    let tx_status = ctx.alice.client.submit_and_await_commit(&tx).await?;

    if !matches!(
        tx_status,
        fuel_core_client::client::types::TransactionStatus::Success { .. }
    ) {
        return Err("collect fee transaction is not successful".into());
    }

    let receipts = match &tx_status {
        fuel_core_client::client::types::TransactionStatus::Success {
            receipts, ..
        } => Some(receipts),
        _ => None,
    };
    let receipts = receipts.ok_or("collect fee transaction doesn't have receipts")?;

    if !receipts
        .iter()
        .any(|receipt| matches!(receipt, Receipt::TransferOut { .. }))
    {
        let msg = format!("TransferOut receipt not found in receipts: {:?}", receipts);
        return Err(msg.into());
    }

    Ok(())
}
