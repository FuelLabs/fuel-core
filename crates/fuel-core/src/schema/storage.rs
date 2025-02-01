use crate::{
    fuel_core_graphql_api::database::ReadView,
    graphql_api::api_service::ReadDatabase,
    require_debug,
    schema::scalars::{
        Bytes32,
        ContractId,
        HexString,
        U32,
    },
};
use async_graphql::{
    Context,
    Object,
    Subscription,
};
use fuel_core_services::stream::Stream;
use fuel_core_types::fuel_types;
use futures::{
    StreamExt,
    TryStreamExt,
};

#[derive(Default)]
pub struct StorageQuery;

#[Object]
impl StorageQuery {
    async fn contract_storage_values(
        &self,
        ctx: &Context<'_>,
        contract_id: ContractId,
        block_height: Option<U32>,
        storage_slots: Vec<Bytes32>,
    ) -> async_graphql::Result<Vec<StorageSlot>> {
        require_debug(ctx)?;

        let view_block_height = if let Some(block_height) = block_height {
            block_height.0.into()
        } else {
            let read_view: &ReadView = ctx.data_unchecked();
            read_view.latest_height()?
        };

        let read_database: &ReadDatabase = ctx.data_unchecked();
        let view_at = read_database.view_at(view_block_height)?;
        let storage_slots = storage_slots
            .into_iter()
            .map(|x| x.into())
            .collect::<Vec<_>>();

        let stream = view_at
            .contract_storage_values(contract_id.into(), storage_slots)
            .map(|result| result.map(|(key, value)| StorageSlot { key, value }))
            .try_collect()
            .await?;

        Ok(stream)
    }
}

#[derive(Default)]
pub struct StorageSubscription;

#[Subscription]
impl StorageSubscription {
    async fn all_storage_slots<'a>(
        &self,
        ctx: &Context<'a>,
        contract_id: ContractId,
    ) -> async_graphql::Result<impl Stream<Item = async_graphql::Result<StorageSlot>> + 'a>
    {
        require_debug(ctx)?;
        let read_view: &ReadView = ctx.data_unchecked();

        let stream = read_view
            .contract_storage_slots(contract_id.into())
            .map(|result| {
                result
                    .map(|(key, value)| StorageSlot { key, value })
                    .map_err(|e| anyhow::anyhow!(e).into())
            });

        Ok(stream)
    }
}

pub struct StorageSlot {
    key: fuel_types::Bytes32,
    value: Vec<u8>,
}

#[Object]
impl StorageSlot {
    async fn key(&self) -> Bytes32 {
        self.key.into()
    }

    async fn value(&self) -> HexString {
        HexString(self.value.clone())
    }
}
