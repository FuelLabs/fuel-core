use crate::fuel_core_graphql_api::{
    api_service::ReadDatabase,
    block_height_subscription,
};
use async_graphql::{
    extensions::{
        Extension,
        ExtensionContext,
        ExtensionFactory,
        NextPrepareRequest,
    },
    Pos,
    Request,
    ServerError,
    ServerResult,
    Value,
};
use async_graphql_value::ConstValue;
use fuel_core_types::fuel_types::BlockHeight;
use std::sync::Arc;
use tokio::time::Duration;

const REQUIRED_FUEL_BLOCK_HEIGHT: &str = "required_fuel_block_height";
pub(crate) const FUEL_BLOCK_HEIGHT_PRECONDITION_FAILED: &str =
    "fuel_block_height_precondition_failed";

/// The extension that implements the logic for checking whether
/// the precondition that REQUIRED_FUEL_BLOCK_HEADER must
/// be higher than the current block height is met.
/// The value of the REQUIRED_FUEL_BLOCK_HEADER is set in
/// the request data by the graphql handler as a value of type
/// `RequiredHeight`.
#[derive(Debug, derive_more::Display, derive_more::From)]
#[display(
    fmt = "RequiredFuelBlockHeightExtension(tolerance: {})",
    tolerance_threshold
)]
pub(crate) struct RequiredFuelBlockHeightExtension {
    tolerance_threshold: u32,
    timeout: Duration,
    block_height_subscriber: block_height_subscription::Subscriber,
}

enum BlockHeightComparison {
    TooFarBehind,
    WithinTolerance,
    Ahead,
}

impl BlockHeightComparison {
    fn from_block_heights(
        required_block_height: &BlockHeight,
        current_block_height: &BlockHeight,
        tolerance_threshold: u32,
    ) -> Self {
        if current_block_height.saturating_add(tolerance_threshold)
            < **required_block_height
        {
            BlockHeightComparison::TooFarBehind
        } else if current_block_height < required_block_height {
            BlockHeightComparison::WithinTolerance
        } else {
            BlockHeightComparison::Ahead
        }
    }
}

impl RequiredFuelBlockHeightExtension {
    pub fn new(
        tolerance_threshold: u32,
        timeout: Duration,
        block_height_subscriber: block_height_subscription::Subscriber,
    ) -> Self {
        Self {
            tolerance_threshold,
            timeout,
            block_height_subscriber,
        }
    }
}

pub(crate) struct RequiredFuelBlockHeightInner {
    tolerance_threshold: u32,
    timeout: Duration,
    block_height_subscriber: block_height_subscription::Subscriber,
}

impl RequiredFuelBlockHeightInner {
    pub fn new(
        tolerance_threshold: u32,
        timeout: Duration,
        block_height_subscriber: &block_height_subscription::Subscriber,
    ) -> Self {
        Self {
            tolerance_threshold,
            timeout,
            block_height_subscriber: block_height_subscriber.clone(),
        }
    }
}

impl ExtensionFactory for RequiredFuelBlockHeightExtension {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(RequiredFuelBlockHeightInner::new(
            self.tolerance_threshold,
            self.timeout,
            &self.block_height_subscriber,
        ))
    }
}

fn get_required_height(request: &Request) -> Option<BlockHeight> {
    let required_fuel_block_height = request.extensions.get(REQUIRED_FUEL_BLOCK_HEIGHT);

    if let Some(ConstValue::Number(required_fuel_block_height)) =
        required_fuel_block_height
    {
        if let Some(required_fuel_block_height) = required_fuel_block_height.as_u64() {
            let required_fuel_block_height: u32 =
                required_fuel_block_height.try_into().unwrap_or(u32::MAX);
            let required_block_height: BlockHeight = required_fuel_block_height.into();

            return Some(required_block_height)
        }
    }
    None
}

#[async_trait::async_trait]
impl Extension for RequiredFuelBlockHeightInner {
    async fn prepare_request(
        &self,
        ctx: &ExtensionContext<'_>,
        request: Request,
        next: NextPrepareRequest<'_>,
    ) -> ServerResult<Request> {
        let required_height = get_required_height(&request);

        let request = next.run(ctx, request).await?;

        if let Some(required_block_height) = required_height {
            let current_block_height =
                self.block_height_subscriber.current_block_height();

            match BlockHeightComparison::from_block_heights(
                &required_block_height,
                &current_block_height,
                self.tolerance_threshold,
            ) {
                BlockHeightComparison::TooFarBehind => {
                    return Err(error_response(
                        &required_block_height,
                        &current_block_height,
                    ));
                }
                BlockHeightComparison::WithinTolerance => {
                    let timeout = self.timeout;
                    match await_block_height(
                        &self.block_height_subscriber,
                        &required_block_height,
                        &timeout,
                    )
                    .await
                    {
                        Ok(()) => {}
                        Err(e) => {
                            let (line, column) = (line!(), column!());
                            tracing::error!(
                                "Failed to wait for the required fuel block height: {}",
                                e
                            );
                            return Err(ServerError::new(
                                "Failed to wait for the required fuel block height",
                                Some(Pos {
                                    line: line as usize,
                                    column: column as usize,
                                }),
                            ));
                        }
                    }
                }
                BlockHeightComparison::Ahead => {}
            }
        };

        let database: &ReadDatabase = ctx.data_unchecked();
        let view = match database.view() {
            Ok(view) => view,
            Err(e) => {
                let (line, column) = (line!(), column!());
                tracing::error!("Failed get the `ReadView`: {}", e);
                return Err(ServerError::new(
                    "Failed get the `ReadView`",
                    Some(Pos {
                        line: line as usize,
                        column: column as usize,
                    }),
                ));
            }
        };

        let request = request.data(view);

        Ok(request)
    }
}

fn error_response(
    required_block_height: &BlockHeight,
    current_block_height: &BlockHeight,
) -> ServerError {
    let mut error = ServerError::new(
        format!(
            "The required fuel block height is higher than the current block height. \
            Required: {}, Current: {}",
            // required_block_height: &BlockHeight, dereference twice to get the
            // corresponding value as u32. This is necessary because the Display
            // implementation for BlockHeight displays values in hexadecimal format.
            **required_block_height,
            // current_fuel_block_height: &BlockHeight, dereference twice to get the
            // corresponding value as u32.
            **current_block_height
        ),
        None,
    );

    if error.extensions.is_none() {
        error.extensions = Some(Default::default());
    }

    error
        .extensions
        .as_mut()
        .expect("Inserted above; qed")
        .set(FUEL_BLOCK_HEIGHT_PRECONDITION_FAILED, Value::Boolean(true));

    error
}

async fn await_block_height(
    block_height_subscriber: &block_height_subscription::Subscriber,
    block_height: &BlockHeight,
    timeout: &Duration,
) -> anyhow::Result<()> {
    tokio::select! {
        biased;
        block_height_res = block_height_subscriber.wait_for_block_height(*block_height) => {
            block_height_res.map_err(|e| {
                anyhow::anyhow!(
                    "Failed to wait for the required fuel block height: {}",
                    e
                )})
        },
        _ = tokio::time::sleep(*timeout) => {
            Err(anyhow::anyhow!(
                "Timeout while waiting for the required fuel block height: {}",
                block_height
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphql_api::extensions::unify_response;
    use async_graphql::Response;
    use std::collections::BTreeMap;

    #[test]
    fn unify_response_method_is_updated() {
        // Given
        let original_error =
            error_response(&BlockHeight::from(100), &BlockHeight::from(99));
        let error_response = Response::from_errors(vec![original_error.clone()]);
        let error_extensions = original_error.extensions.unwrap();

        // Hack: `ErrorExtensionValues` doesn't allow to get the access to the inner map.
        let bytes = serde_json::to_string(&error_extensions).unwrap();
        let error_extensions: BTreeMap<String, Value> =
            serde_json::from_str(&bytes).unwrap();

        // When
        let unified_error_response = unify_response(error_response);

        // Then
        assert_eq!(unified_error_response.extensions, error_extensions);
    }
}
