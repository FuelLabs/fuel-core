use crate::fuel_core_graphql_api::extensions::{
    chain_state_info::{
        CURRENT_CONSENSUS_PARAMETERS_VERSION,
        CURRENT_FUEL_BLOCK_HEIGHT,
        CURRENT_STF_VERSION,
    },
    required_fuel_block_height::FUEL_BLOCK_HEIGHT_PRECONDITION_FAILED,
};
use async_graphql::Response;

pub(crate) mod chain_state_info;
pub(crate) mod metrics;
pub(crate) mod required_fuel_block_height;
pub(crate) mod validation;

// In the case of a successful query, we return the information below on
// the `response.extensions` level.
// But in the case of the error, `async_graphql` returns information from extensions
// in the form of `response.errors.extensions`.
// While both variants are valid, we still want SDK to be able to
// process information in one unified way.
// For that, we duplicate information that SDK expects to `response.extensions` from errors.
pub fn unify_response(mut response: Response) -> Response {
    for error in &response.errors {
        if let Some(extensions) = &error.extensions {
            if let Some(value) = extensions.get(FUEL_BLOCK_HEIGHT_PRECONDITION_FAILED) {
                response.extensions.insert(
                    FUEL_BLOCK_HEIGHT_PRECONDITION_FAILED.to_string(),
                    value.clone(),
                );
            }
            if let Some(value) = extensions.get(CURRENT_STF_VERSION) {
                response
                    .extensions
                    .insert(CURRENT_STF_VERSION.to_string(), value.clone());
            }
            if let Some(value) = extensions.get(CURRENT_CONSENSUS_PARAMETERS_VERSION) {
                response.extensions.insert(
                    CURRENT_CONSENSUS_PARAMETERS_VERSION.to_string(),
                    value.clone(),
                );
            }
            if let Some(value) = extensions.get(CURRENT_FUEL_BLOCK_HEIGHT) {
                response
                    .extensions
                    .insert(CURRENT_FUEL_BLOCK_HEIGHT.to_string(), value.clone());
            }
        }
    }

    response
}
