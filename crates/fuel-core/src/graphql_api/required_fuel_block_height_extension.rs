use super::{
    block_height_subscription,
    database::{
        ReadDatabase,
        ReadView,
    },
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

use tokio::time::Duration;

const REQUIRED_FUEL_BLOCK_HEIGHT: &str = "required_fuel_block_height";
const CURRENT_FUEL_BLOCK_HEIGHT: &str = "current_fuel_block_height";
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
    min_timeout: Duration,
    block_height_subscriber: block_height_subscription::Subscriber,
}

enum BlockHeightComparison {
    TooFarBehind,
    WithinTolerance(u32),
    Ahead,
}

impl BlockHeightComparison {
    fn from_block_heights(
        required_block_height: &BlockHeight,
        current_block_height: &BlockHeight,
        tolerance_threshold: u32,
    ) -> Self {
        if **current_block_height
            < required_block_height.saturating_sub(tolerance_threshold)
        {
            BlockHeightComparison::TooFarBehind
        } else if current_block_height < required_block_height {
            BlockHeightComparison::WithinTolerance(
                required_block_height.saturating_sub(**current_block_height),
            )
        } else {
            BlockHeightComparison::Ahead
        }
    }
}

impl RequiredFuelBlockHeightExtension {
    pub fn new(
        tolerance_threshold: u32,
        min_timeout: Duration,
        block_height_subscriber: block_height_subscription::Subscriber,
    ) -> Self {
        Self {
            tolerance_threshold,
            // : make it configurable
            min_timeout,
            block_height_subscriber,
        }
    }
}

pub(crate) struct RequiredFuelBlockHeightInner {
    required_height: OnceLock<BlockHeight>,
    tolerance_threshold: u32,
    min_timeout: Duration,
    block_height_subscriber: block_height_subscription::Subscriber,
}

impl RequiredFuelBlockHeightInner {
    pub fn new(
        tolerance_threshold: u32,
        min_timeout: Duration,
        block_height_subscriber: &block_height_subscription::Subscriber,
    ) -> Self {
        Self {
            required_height: OnceLock::new(),
            tolerance_threshold,
            min_timeout,
            block_height_subscriber: block_height_subscriber.clone(),
        }
    }
}

impl ExtensionFactory for RequiredFuelBlockHeightExtension {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(RequiredFuelBlockHeightInner::new(
            self.tolerance_threshold,
            self.min_timeout,
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
        // TODO: https://github.com/FuelLabs/fuel-core/issues/2713.
        // Should we really need to fetch a view here, or can we just recover
        // the current block height from the offchain worker via the block
        // height subscription handle?
        let view: &ReadView = ctx.data_unchecked();

        let Ok(current_block_height) = view.latest_block_height() else {
            let (line, column) = (line!(), column!());
            return Response::from_errors(vec![ServerError::new(
                "Internal server error while fetching the current block height",
                Some(Pos {
                    line: line as usize,
                    column: column as usize,
                }),
            )]);
        };
        if let Some(required_block_height) = self.required_height.get() {
            match BlockHeightComparison::from_block_heights(
                &required_block_height,
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
                BlockHeightComparison::WithinTolerance(blocks) => {
                    let timeout = self
                        .min_timeout
                        .saturating_add(Duration::from_secs(blocks.into()));
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

        let database: &ReadDatabase = ctx.data_unchecked();
        let view = match database.view() {
            Ok(view) => view,
            Err(e) => {
                let (line, column) = (line!(), column!());
                return Response::from_errors(vec![ServerError::new(
                    e.to_string(),
                    Some(Pos {
                        line: line as usize,
                        column: column as usize,
                    }),
                )]);
            }
        };
        let current_block_height = match view.latest_block_height() {
            Ok(height) => height,
            Err(e) => {
                let (line, column) = (line!(), column!());
                return Response::from_errors(vec![ServerError::new(
                    e.to_string(),
                    Some(Pos {
                        line: line as usize,
                        column: column as usize,
                    }),
                )]);
            }
        };
        // Dereference to display the value in decimal base.
        let current_block_height: u32 = *current_block_height;
        response.extensions.insert(
            CURRENT_FUEL_BLOCK_HEIGHT.to_string(),
            Value::Number(current_block_height.into()),
        );
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
        CURRENT_FUEL_BLOCK_HEIGHT.to_string(),
        Value::Number((**current_block_height).into()),
    );
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
