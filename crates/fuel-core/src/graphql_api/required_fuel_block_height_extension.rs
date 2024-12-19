use super::database::ReadView;
use crate::fuel_core_graphql_api::api_service::{
    CURRENT_FUEL_BLOCK_HEIGHT_HEADER,
    REQUIRED_BLOCK_HEIGHT_CHECK_FAILED,
    REQUIRED_FUEL_BLOCK_HEIGHT_HEADER,
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
    Value,
};
use async_graphql_value::ConstValue;
use fuel_core_types::fuel_types::BlockHeight;
use std::sync::{
    Arc,
    OnceLock,
};

/// The extension that implements the logic for checking whether
/// the precondition that REQUIRED_FUEL_BLOCK_HEADER must
/// be higher than the current block height is met.
/// The value of the REQUIRED_FUEL_BLOCK_HEADER is set in
/// the request data by the graphql handler as a value of type
/// `RequiredHeight`.
#[derive(Debug, derive_more::Display, derive_more::From)]
pub(crate) struct RequiredFuelBlockHeightExtension;

impl RequiredFuelBlockHeightExtension {
    pub fn new() -> Self {
        Self
    }
}

pub(crate) struct RequiredFuelBlockHeightInner {
    required_height: OnceLock<BlockHeight>,
}

impl RequiredFuelBlockHeightInner {
    pub fn new() -> Self {
        Self {
            required_height: OnceLock::new(),
        }
    }
}

impl ExtensionFactory for RequiredFuelBlockHeightExtension {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(RequiredFuelBlockHeightInner::new())
    }
}

#[async_trait::async_trait]
impl Extension for RequiredFuelBlockHeightInner {
    async fn prepare_request(
        &self,
        ctx: &ExtensionContext<'_>,
        request: Request,
        next: NextPrepareRequest<'_>,
    ) -> ServerResult<Request> {
        let required_fuel_block_height =
            request.extensions.get(REQUIRED_FUEL_BLOCK_HEIGHT_HEADER);

        if let Some(ConstValue::Number(required_fuel_block_height)) =
            required_fuel_block_height
        {
            if let Some(required_fuel_block_height) = required_fuel_block_height.as_u64()
            {
                let required_fuel_block_height: u32 =
                    required_fuel_block_height.try_into().unwrap_or(u32::MAX);
                let required_block_height: BlockHeight =
                    required_fuel_block_height.into();
                self.required_height
                    .set(required_block_height)
                    .expect("`prepare_request` called only once; qed");
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
        let view: &ReadView = ctx.data_unchecked();

        let current_block_height = view.latest_block_height();

        if let Some(required_block_height) = self.required_height.get() {
            if let Ok(current_block_height) = view.latest_block_height() {
                if *required_block_height > current_block_height {
                    let (line, column) = (line!(), column!());
                    let mut response = Response::from_errors(vec![ServerError::new(
                        format!(
                            "The required fuel block height is higher than the current block height. \
                            Required: {}, Current: {}",
                            required_block_height,
                            current_block_height
                        ),
                        Some(Pos {
                            line: line as usize,
                            column: column as usize,
                        }),
                    )]);

                    response.extensions.insert(
                        REQUIRED_BLOCK_HEIGHT_CHECK_FAILED.to_string(),
                        Value::Boolean(true),
                    );

                    response.extensions.insert(
                        CURRENT_FUEL_BLOCK_HEIGHT_HEADER.to_string(),
                        Value::Number((*current_block_height).into()),
                    );

                    return response
                }
            }
        }

        let mut response = next.run(ctx, operation_name).await;

        let current_block_height = view.latest_block_height();

        if let Ok(current_block_height) = current_block_height {
            let current_block_height: u32 = *current_block_height;
            response.extensions.insert(
                CURRENT_FUEL_BLOCK_HEIGHT_HEADER.to_string(),
                Value::Number(current_block_height.into()),
            );
        } else {
            tracing::error!("Failed to get the current block height");
        }

        response
    }
}
