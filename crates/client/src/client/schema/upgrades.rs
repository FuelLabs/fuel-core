use crate::client::schema::schema;

use crate::client::{
    schema::{
        chain::ConsensusParameters,
        primitives::HexString,
    },
    ConversionError,
};

use fuel_core_types::fuel_vm::UploadedBytecode as VmUploadedBytecode;

#[derive(cynic::QueryVariables, Debug)]
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

#[derive(cynic::QueryVariables, Debug)]
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

#[derive(cynic::QueryVariables, Debug)]
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

// GrapqhQL translation

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
