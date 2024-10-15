use crate::{
    fuel_core_graphql_api::query_costs,
    graphql_api::IntoApiResult,
    schema::{
        scalars::{
            BlobId,
            HexString,
        },
        ReadViewProvider,
    },
};
use async_graphql::{
    Context,
    Object,
};
use fuel_core_storage::{
    not_found,
    tables::BlobData,
};
use fuel_core_types::fuel_types;

pub struct Blob(fuel_types::BlobId);

#[Object]
impl Blob {
    async fn id(&self) -> BlobId {
        self.0.into()
    }

    #[graphql(complexity = "query_costs().bytecode_read")]
    async fn bytecode(&self, ctx: &Context<'_>) -> async_graphql::Result<HexString> {
        let query = ctx.read_view()?;
        query
            .blob_bytecode(self.0)
            .map(HexString)
            .map_err(async_graphql::Error::from)
    }
}

impl From<fuel_types::BlobId> for Blob {
    fn from(value: fuel_types::BlobId) -> Self {
        Self(value)
    }
}

#[derive(Default)]
pub struct BlobQuery;

#[Object]
impl BlobQuery {
    #[graphql(complexity = "query_costs().storage_read + child_complexity")]
    async fn blob(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "ID of the Blob")] id: BlobId,
    ) -> async_graphql::Result<Option<Blob>> {
        let query = ctx.read_view()?;
        query
            .blob_exists(id.0)
            .and_then(|blob_exists| {
                if blob_exists {
                    Ok(id.0)
                } else {
                    Err(not_found!(BlobData))
                }
            })
            .into_api_result()
    }
}
