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

use tokio::time::Duration;

use crate::graphql_api::block_height_subscription;

const REQUIRED_FUEL_BLOCK_HEIGHT: &str = "required_fuel_block_height";
const FUEL_BLOCK_HEIGHT_PRECONDITION_FAILED: &str =
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
    required_height: OnceLock<BlockHeight>,
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
            required_height: OnceLock::new(),
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

#[async_trait::async_trait]
impl Extension for RequiredFuelBlockHeightInner {
    async fn prepare_request(
        &self,
        ctx: &ExtensionContext<'_>,
        request: Request,
        next: NextPrepareRequest<'_>,
    ) -> ServerResult<Request> {
        let required_fuel_block_height =
            request.extensions.get(REQUIRED_FUEL_BLOCK_HEIGHT);

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
        if let Some(required_block_height) = self.required_height.get() {
            let current_block_height =
                self.block_height_subscriber.current_block_height();

            match BlockHeightComparison::from_block_heights(
                required_block_height,
                &current_block_height,
                self.tolerance_threshold,
            ) {
                BlockHeightComparison::TooFarBehind => {
                    let (line, column) = (line!(), column!());
                    return error_response(
                        required_block_height,
                        &current_block_height,
                        line,
                        column,
                    );
                }
                BlockHeightComparison::WithinTolerance => {
                    let timeout = self.timeout;
                    match await_block_height(
                        &self.block_height_subscriber,
                        required_block_height,
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
                            return Response::from_errors(vec![ServerError::new(
                                "Failed to wait for the required fuel block height",
                                Some(Pos {
                                    line: line as usize,
                                    column: column as usize,
                                }),
                            )]);
                        }
                    }
                }
                BlockHeightComparison::Ahead => {}
            }
        };

        let mut response = next.run(ctx, operation_name).await;

        if self.required_height.get().is_some() {
            response.extensions.insert(
                FUEL_BLOCK_HEIGHT_PRECONDITION_FAILED.to_string(),
                Value::Boolean(false),
            );
        }

        response
    }
}

fn error_response(
    required_block_height: &BlockHeight,
    current_block_height: &BlockHeight,
    line: u32,
    column: u32,
) -> Response {
    let mut response = Response::from_errors(vec![ServerError::new(
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
        Some(Pos {
            line: line as usize,
            column: column as usize,
        }),
    )]);

    response.extensions.insert(
        FUEL_BLOCK_HEIGHT_PRECONDITION_FAILED.to_string(),
        Value::Boolean(true),
    );

    response
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
