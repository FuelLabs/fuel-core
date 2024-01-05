use crate::{
    database::Database,
    fuel_core_graphql_api::{
        database::OnChainView,
        ports::{
            DatabaseBlocks,
            DatabaseChain,
            DatabaseContracts,
            DatabaseMessages,
            OnChainDatabase,
        },
    },
};
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IntoBoxedIter,
        IterDirection,
    },
    not_found,
    transactional::AtomicView,
    Error as StorageError,
    Result as StorageResult,
};
use fuel_core_txpool::types::ContractId;
use fuel_core_types::{
    blockchain::primitives::{
        BlockId,
        DaBlockHeight,
    },
    entities::message::Message,
    fuel_tx::AssetId,
    fuel_types::{
        BlockHeight,
        Nonce,
    },
    services::graphql_api::ContractBalance,
};
use std::sync::Arc;

impl DatabaseBlocks for Database {
    fn block_id(&self, height: &BlockHeight) -> StorageResult<BlockId> {
        self.get_block_id(height)
            .and_then(|height| height.ok_or(not_found!("BlockId")))
    }

    fn blocks_ids(
        &self,
        start: Option<BlockHeight>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<(BlockHeight, BlockId)>> {
        self.all_block_ids(start, direction)
            .map(|result| result.map_err(StorageError::from))
            .into_boxed()
    }

    fn ids_of_latest_block(&self) -> StorageResult<(BlockHeight, BlockId)> {
        self.ids_of_latest_block()
            .transpose()
            .ok_or(not_found!("BlockId"))?
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
    fn chain_name(&self) -> StorageResult<String> {
        pub const DEFAULT_NAME: &str = "Fuel.testnet";

        Ok(self
            .get_chain_name()?
            .unwrap_or_else(|| DEFAULT_NAME.to_string()))
    }

    fn da_height(&self) -> StorageResult<DaBlockHeight> {
        #[cfg(feature = "relayer")]
        {
            use fuel_core_relayer::ports::RelayerDb;
            self.get_finalized_da_height()
        }
        #[cfg(not(feature = "relayer"))]
        {
            Ok(0u64.into())
        }
    }
}

impl OnChainDatabase for Database {}

impl AtomicView<OnChainView> for Database {
    fn view_at(&self, _: BlockHeight) -> StorageResult<OnChainView> {
        unimplemented!(
            "Unimplemented until of the https://github.com/FuelLabs/fuel-core/issues/451"
        )
    }

    fn latest_view(&self) -> OnChainView {
        // TODO: https://github.com/FuelLabs/fuel-core/issues/1581
        Arc::new(self.clone())
    }
}
