use super::{
    import_task::ImportTable,
    Handler,
};
use crate::database::{
    balances::BalancesInitializer,
    database_description::on_chain::OnChain,
    state::StateInitializer,
    GenesisDatabase,
};
use anyhow::anyhow;
use fuel_core_chain_config::TableEntry;
use fuel_core_storage::{
    tables::{
        merkle::{
            FuelBlockMerkleData,
            FuelBlockMerkleMetadata,
        },
        Coins,
        ContractsAssets,
        ContractsLatestUtxo,
        ContractsRawCode,
        ContractsState,
        Messages,
        ProcessedTransactions,
    },
    transactional::StorageTransaction,
    StorageAsMut,
};
use fuel_core_types::{
    self,
    blockchain::primitives::DaBlockHeight,
    entities::{
        coins::coin::Coin,
        Message,
    },
    fuel_types::BlockHeight,
    fuel_vm::BlobData,
};

impl ImportTable for Handler<Coins, Coins> {
    type TableInSnapshot = Coins;
    type TableBeingWritten = Coins;
    type DbDesc = OnChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase>,
    ) -> anyhow::Result<()> {
        group.into_iter().try_for_each(|coin| {
            init_coin(tx, &coin, self.block_height)?;
            Ok(())
        })
    }
}

impl ImportTable for Handler<Messages, Messages> {
    type TableInSnapshot = Messages;
    type TableBeingWritten = Messages;
    type DbDesc = OnChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase>,
    ) -> anyhow::Result<()> {
        group
            .into_iter()
            .try_for_each(|message| init_da_message(tx, message, self.da_block_height))
    }
}

impl ImportTable for Handler<ProcessedTransactions, ProcessedTransactions> {
    type TableInSnapshot = ProcessedTransactions;
    type TableBeingWritten = ProcessedTransactions;
    type DbDesc = OnChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase>,
    ) -> anyhow::Result<()> {
        group.into_iter().try_for_each(|transaction| {
            tx.storage_as_mut::<ProcessedTransactions>()
                .insert(&transaction.key, &transaction.value)
                .map(|_| ())
        })?;
        Ok(())
    }
}

impl ImportTable for Handler<BlobData, BlobData> {
    type TableInSnapshot = BlobData;
    type TableBeingWritten = BlobData;
    type DbDesc = OnChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase>,
    ) -> anyhow::Result<()> {
        group
            .into_iter()
            .try_for_each(|entry| init_blob_payload(tx, &entry))
    }
}

impl ImportTable for Handler<ContractsRawCode, ContractsRawCode> {
    type TableInSnapshot = ContractsRawCode;
    type TableBeingWritten = ContractsRawCode;
    type DbDesc = OnChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase>,
    ) -> anyhow::Result<()> {
        group.into_iter().try_for_each(|contract| {
            init_contract_raw_code(tx, &contract)?;
            Ok::<(), anyhow::Error>(())
        })
    }
}

impl ImportTable for Handler<ContractsLatestUtxo, ContractsLatestUtxo> {
    type TableInSnapshot = ContractsLatestUtxo;
    type TableBeingWritten = ContractsLatestUtxo;
    type DbDesc = OnChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase>,
    ) -> anyhow::Result<()> {
        group.into_iter().try_for_each(|contract| {
            init_contract_latest_utxo(tx, &contract, self.block_height)?;
            Ok::<(), anyhow::Error>(())
        })
    }
}

impl ImportTable for Handler<ContractsState, ContractsState> {
    type TableInSnapshot = ContractsState;
    type TableBeingWritten = ContractsState;
    type DbDesc = OnChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase>,
    ) -> anyhow::Result<()> {
        tx.update_contract_states(group)?;
        Ok(())
    }
}

impl ImportTable for Handler<ContractsAssets, ContractsAssets> {
    type TableInSnapshot = ContractsAssets;
    type TableBeingWritten = ContractsAssets;
    type DbDesc = OnChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase>,
    ) -> anyhow::Result<()> {
        tx.update_contract_balances(group)?;
        Ok(())
    }
}

impl ImportTable for Handler<FuelBlockMerkleData, FuelBlockMerkleData> {
    type TableInSnapshot = FuelBlockMerkleData;
    type TableBeingWritten = FuelBlockMerkleData;
    type DbDesc = OnChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        let blocks = group
            .iter()
            .map(|TableEntry { key, value, .. }| (key, value));
        for (height, block) in blocks {
            tx.storage::<FuelBlockMerkleData>().insert(height, block)?;
        }
        Ok(())
    }
}

impl ImportTable for Handler<FuelBlockMerkleMetadata, FuelBlockMerkleMetadata> {
    type TableInSnapshot = FuelBlockMerkleMetadata;
    type TableBeingWritten = FuelBlockMerkleMetadata;
    type DbDesc = OnChain;

    fn process(
        &mut self,
        group: Vec<TableEntry<Self::TableInSnapshot>>,
        tx: &mut StorageTransaction<&mut GenesisDatabase<Self::DbDesc>>,
    ) -> anyhow::Result<()> {
        let blocks = group
            .iter()
            .map(|TableEntry { key, value, .. }| (key, value));
        for (height, metadata) in blocks {
            tx.storage::<FuelBlockMerkleMetadata>()
                .insert(height, metadata)?;
        }
        Ok(())
    }
}

fn init_coin(
    transaction: &mut StorageTransaction<&mut GenesisDatabase>,
    coin: &TableEntry<Coins>,
    height: BlockHeight,
) -> anyhow::Result<()> {
    let utxo_id = coin.key;

    let compressed_coin = Coin {
        utxo_id,
        owner: *coin.value.owner(),
        amount: *coin.value.amount(),
        asset_id: *coin.value.asset_id(),
        tx_pointer: *coin.value.tx_pointer(),
    }
    .compress();

    // ensure coin can't point to blocks in the future
    let coin_height = coin.value.tx_pointer().block_height();
    if coin_height > height {
        return Err(anyhow!(
            "coin tx_pointer height ({coin_height}) cannot be greater than genesis block ({height})"
        ));
    }

    if transaction
        .storage::<Coins>()
        .replace(&utxo_id, &compressed_coin)?
        .is_some()
    {
        return Err(anyhow!("Coin should not exist"));
    }

    Ok(())
}

fn init_contract_latest_utxo(
    transaction: &mut StorageTransaction<&mut GenesisDatabase>,
    entry: &TableEntry<ContractsLatestUtxo>,
    height: BlockHeight,
) -> anyhow::Result<()> {
    let contract_id = entry.key;

    if entry.value.tx_pointer().block_height() > height {
        return Err(anyhow!(
            "contract tx_pointer cannot be greater than genesis block"
        ));
    }

    if transaction
        .storage::<ContractsLatestUtxo>()
        .replace(&contract_id, &entry.value)?
        .is_some()
    {
        return Err(anyhow!("Contract utxo should not exist"));
    }

    Ok(())
}

fn init_blob_payload(
    transaction: &mut StorageTransaction<&mut GenesisDatabase>,
    entry: &TableEntry<BlobData>,
) -> anyhow::Result<()> {
    let payload = entry.value.as_ref();
    let blob_id = entry.key;

    // insert blob payload
    if transaction
        .storage::<BlobData>()
        .replace(&blob_id, payload)?
        .is_some()
    {
        return Err(anyhow!("Blob should not exist"));
    }

    Ok(())
}

fn init_contract_raw_code(
    transaction: &mut StorageTransaction<&mut GenesisDatabase>,
    entry: &TableEntry<ContractsRawCode>,
) -> anyhow::Result<()> {
    let contract = entry.value.as_ref();
    let contract_id = entry.key;

    // insert contract code
    if transaction
        .storage::<ContractsRawCode>()
        .replace(&contract_id, contract)?
        .is_some()
    {
        return Err(anyhow!("Contract code should not exist"));
    }

    Ok(())
}

fn init_da_message(
    transaction: &mut StorageTransaction<&mut GenesisDatabase>,
    msg: TableEntry<Messages>,
    da_height: DaBlockHeight,
) -> anyhow::Result<()> {
    let message: Message = msg.value;

    if message.da_height() > da_height {
        return Err(anyhow!(
            "message da_height cannot be greater than genesis da block height"
        ));
    }

    if transaction
        .storage::<Messages>()
        .replace(message.id(), &message)?
        .is_some()
    {
        return Err(anyhow!("Message should not exist"));
    }

    Ok(())
}
