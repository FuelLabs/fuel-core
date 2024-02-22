use crate::{
    database::Database,
    fuel_core_graphql_api::ports::{
        DatabaseBlocks,
        DatabaseChain,
        DatabaseContracts,
        DatabaseMessages,
        OnChainDatabase,
    },
};
use fuel_core_importer::ports::ImporterDatabase;
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IntoBoxedIter,
        IterDirection,
    },
    not_found,
    tables::FuelBlocks,
    Error as StorageError,
    Result as StorageResult,
};
use fuel_core_txpool::types::ContractId;
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        primitives::DaBlockHeight,
    },
    entities::message::Message,
    fuel_tx::AssetId,
    fuel_types::{
        BlockHeight,
        Nonce,
    },
    services::graphql_api::ContractBalance,
};

impl DatabaseBlocks for Database {
    fn blocks(
        &self,
        height: Option<BlockHeight>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<CompressedBlock>> {
        self.iter_all_by_start::<FuelBlocks>(height.as_ref(), Some(direction))
            .map(|result| result.map(|(_, block)| block))
            .into_boxed()
    }

    fn latest_height(&self) -> StorageResult<BlockHeight> {
        self.latest_block_height()
            .transpose()
            .ok_or(not_found!("BlockHeight"))?
    }
}

impl DatabaseMessages for Database {
    fn all_messages(
        &self,
        start_message_id: Option<Nonce>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<Message>> {
        self.all_messages(start_message_id, Some(direction))
            .map(|result| result.map_err(StorageError::from))
            .into_boxed()
    }

    fn message_is_spent(&self, nonce: &Nonce) -> StorageResult<bool> {
        self.message_is_spent(nonce)
    }

    fn message_exists(&self, nonce: &Nonce) -> StorageResult<bool> {
        self.message_exists(nonce)
    }
}

impl DatabaseContracts for Database {
    fn contract_balances(
        &self,
        contract: ContractId,
        start_asset: Option<AssetId>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<ContractBalance>> {
        self.contract_balances(contract, start_asset, Some(direction))
            .map(move |result| {
                result
                    .map_err(StorageError::from)
                    .map(|(asset_id, amount)| ContractBalance {
                        owner: contract,
                        amount,
                        asset_id,
                    })
            })
            .into_boxed()
    }
}

impl DatabaseChain for Database {
    fn da_height(&self) -> StorageResult<DaBlockHeight> {
        self.latest_compressed_block()?
            .map(|block| block.header().da_height)
            .ok_or(not_found!("DaBlockHeight"))
    }
}

impl OnChainDatabase for Database {}
