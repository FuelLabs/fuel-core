use crate::client::schema::schema;

use crate::client::{
    ConversionError,
    schema::{
        chain::{
            ConsensusParameters,
            ConsensusParametersLegacy,
        },
        primitives::HexString,
    },
};

use fuel_core_types::fuel_vm::UploadedBytecode as VmUploadedBytecode;

#[derive(cynic::QueryVariables, Debug, Clone)]
pub struct ConsensusParametersByVersionArgs {
    pub version: i32,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "ConsensusParametersByVersionArgs"
)]
pub struct ConsensusParametersByVersionQuery {
    #[arguments(version: $version)]
    pub consensus_parameters: Option<ConsensusParameters>,
}

/// Legacy variant for nodes older than v0.48.0 that do not expose
/// `maxStorageSlotLength` or the storage gas-cost fields.
#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "ConsensusParametersByVersionArgs"
)]
pub struct ConsensusParametersByVersionQueryLegacy {
    #[arguments(version: $version)]
    pub consensus_parameters: Option<ConsensusParametersLegacy>,
}

#[derive(cynic::QueryVariables, Debug, Clone)]
pub struct StateTransitionBytecodeByVersionArgs {
    pub version: i32,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "StateTransitionBytecodeByVersionArgs"
)]
pub struct StateTransitionBytecodeByVersionQuery {
    #[arguments(version: $version)]
    pub state_transition_bytecode_by_version: Option<StateTransitionBytecode>,
}

#[derive(cynic::QueryVariables, Debug, Clone)]
pub struct StateTransitionBytecodeByRootArgs {
    pub root: HexString,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(
    schema_path = "./assets/schema.sdl",
    graphql_type = "Query",
    variables = "StateTransitionBytecodeByRootArgs"
)]
pub struct StateTransitionBytecodeByRootQuery {
    #[arguments(root: $root)]
    pub state_transition_bytecode_by_root: Option<StateTransitionBytecode>,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct StateTransitionBytecode {
    pub root: HexString,
    pub bytecode: UploadedBytecode,
}

#[derive(cynic::QueryFragment, Clone, Debug)]
#[cynic(schema_path = "./assets/schema.sdl")]
pub struct UploadedBytecode {
    pub bytecode: HexString,
    pub uploaded_subsections_number: Option<i32>,
    pub completed: bool,
}

// GraphQL translation

impl TryFrom<UploadedBytecode> for VmUploadedBytecode {
    type Error = ConversionError;
    fn try_from(value: UploadedBytecode) -> Result<Self, Self::Error> {
        Ok(match value.completed {
            true => Self::Completed(value.bytecode.into()),
            false => Self::Uncompleted {
                bytecode: value.bytecode.into(),
                uploaded_subsections_number: value
                    .uploaded_subsections_number
                    .ok_or_else(|| {
                        ConversionError::MissingField(
                            "uploaded_subsections_number".to_string(),
                        )
                    })?
                    .try_into()?,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Snapshot test for the legacy consensus-parameters query.
    /// If any of the fields absent from pre-0.48.0 nodes sneak back in, this fails.
    #[test]
    fn consensus_parameters_legacy_query_output() {
        use cynic::QueryBuilder;
        let operation = ConsensusParametersByVersionQueryLegacy::build(
            ConsensusParametersByVersionArgs { version: 0 },
        );
        insta::assert_snapshot!(
            "consensus_parameters_legacy_query_output",
            operation.query
        );

        assert!(
            !operation.query.contains("maxStorageSlotLength"),
            "legacy query must not request maxStorageSlotLength"
        );
        assert!(
            !operation.query.contains("storageReadCold"),
            "legacy query must not request storageReadCold"
        );
        assert!(
            !operation.query.contains("storageReadHot"),
            "legacy query must not request storageReadHot"
        );
        assert!(
            !operation.query.contains("storageWrite"),
            "legacy query must not request storageWrite"
        );
        assert!(
            !operation.query.contains("storageClear"),
            "legacy query must not request storageClear"
        );
    }
}
