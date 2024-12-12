use crate::graphql_api::{
    api_service::CURRENT_FUEL_BLOCK_HEIGHT_HEADER,
    database::ReadDatabase,
};
use async_graphql::{
    extensions::{
        Extension,
        ExtensionContext,
        ExtensionFactory,
        NextExecute,
        NextPrepareRequest,
    },
    Pos,
    Request,
    Response,
    ServerError,
    ServerResult,
};
use fuel_core_types::fuel_types::BlockHeight;
use std::{
    any::TypeId,
    sync::Arc,
};
use tokio::sync::Mutex;

use super::api_service::REQUIRED_FUEL_BLOCK_HEIGHT_HEADER;

/// The extension that adds the `ReadView` to the request context.
/// It guarantees that the request works with the one view of the database,
/// and external database modification cannot affect the result.
#[derive(Debug, derive_more::Display, derive_more::From)]
pub(crate) struct RequiredFuelBlockHeightExtension;

impl RequiredFuelBlockHeightExtension {
    pub fn new() -> Self {
        Self
    }
}

impl ExtensionFactory for RequiredFuelBlockHeightExtension {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(RequiredFuelBlockHeightExtension::new())
    }
}

pub(crate) struct RequiredFuelBlockHeightTooFarInTheFuture;

#[async_trait::async_trait]
impl Extension for RequiredFuelBlockHeightExtension {
    async fn prepare_request(
        &self,
        ctx: &ExtensionContext<'_>,
        request: Request,
        next: NextPrepareRequest<'_>,
    ) -> ServerResult<Request> {
        let database: &ReadDatabase = ctx.data_unchecked();
        let required_fuel_block_height_header_value = request
            .extensions
            .get(REQUIRED_FUEL_BLOCK_HEIGHT_HEADER)
            .map(|value| match value {
                async_graphql::Value::Number(number) => {
                    // Safety: The value was constructed
                    BlockHeight::new(
                        number
                            .as_u64()
                            .and_then(|n| n.try_into().ok())
                            .expect("The REQUIRED_FUEL_BLOCK_HEIGHT_HEADER value has been constructed from a u64 value"),
                    )
                }
                _ => panic!("The REQUIRED_FUEL_BLOCK_HEIGHT_HEADER value has been constructed from a u64 value"),
            });

        let latest_known_block_height = database
            .view()
            .and_then(|view| view.latest_block_height())
            .map_err(|e| {
                let (line, column) = (line!(), column!());
                ServerError::new(
                    e.to_string(),
                    Some(Pos {
                        line: line as usize,
                        column: column as usize,
                    }),
                )
            })?;

        {
            // At this point, the query_data in the ExtensionContext is empty.
            // See https://github.com/async-graphql/async-graphql/blob/7f1791488463d4e9c5adcd543962173e2f6cbd34/src/schema.rs#L521
            // We need to fetch the mutable location to store the current fuel block height
            // directly from request.data
            let mut current_block_height = request
            .data
            .get(&TypeId::of::<Arc<Mutex<Option<BlockHeight>>>>())
            .and_then(|data| { println!("Request was here"); data.downcast_ref::<Arc<Mutex<Option<BlockHeight>>>>()})
            .expect("Data to store current fuel block height was set when forming the request")
            .lock()
            .await;

            *current_block_height = Some(latest_known_block_height);
        }

        if let Some(required_fuel_block_height) = required_fuel_block_height_header_value
        {
            if required_fuel_block_height > latest_known_block_height {
                return Err(ServerError {
                    message: "".to_string(),
                    locations: vec![],
                    source: Some(Arc::new(RequiredFuelBlockHeightTooFarInTheFuture)),
                    path: vec![],
                    extensions: None,
                });
            }
        }

        next.run(ctx, request).await
    }

    async fn execute(
        &self,
        ctx: &ExtensionContext<'_>,
        operation_name: Option<&str>,
        next: NextExecute<'_>,
    ) -> Response {
        // The query data has been referenced in the Extension context after
        // Self::prepare_request has been executed.
        // See https://github.com/async-graphql/async-graphql/blob/7f1791488463d4e9c5adcd543962173e2f6cbd34/src/schema.rs#L845.
        // We can fetch the value of the current_fuel_block_height from the extension

        let current_block_height = ctx
            .query_data
            .and_then(|data| data
            .get(&TypeId::of::<Arc<Mutex<Option<BlockHeight>>>>()))
            .and_then(|data| data.downcast_ref::<Arc<Mutex<Option<BlockHeight>>>>())
            .expect("Data to store current fuel block height was set when forming the request")
            .lock()
            .await
            .expect("Data to store current fuel block height was set when preparing the request");

        println!("Current fuel block height: {:?}", current_block_height);
        let mut result = next.run(ctx, operation_name).await;

        result.http_headers.append(
            CURRENT_FUEL_BLOCK_HEIGHT_HEADER,
            (*current_block_height).into(),
        );
        result
    }
}
