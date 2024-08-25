use crate::{
    graphql_api::{
        api_service::ConsensusProvider,
        IntoApiResult,
        QUERY_COSTS,
    },
    query::UpgradeQueryData,
    schema::{
        chain::ConsensusParameters,
        scalars::HexString,
        ReadViewProvider,
    },
};
use async_graphql::{
    Context,
    Object,
    SimpleObject,
};
use fuel_core_types::{
    blockchain::header::{
        ConsensusParametersVersion,
        StateTransitionBytecodeVersion,
    },
    fuel_types,
    fuel_vm::UploadedBytecode as StorageUploadedBytecode,
};

#[derive(Default)]
pub struct UpgradeQuery;

#[Object]
impl UpgradeQuery {
    #[graphql(complexity = "QUERY_COSTS.storage_read + child_complexity")]
    async fn consensus_parameters(
        &self,
        ctx: &Context<'_>,
        version: ConsensusParametersVersion,
    ) -> async_graphql::Result<ConsensusParameters> {
        let params = ctx
            .data_unchecked::<ConsensusProvider>()
            .consensus_params_at_version(&version)?;

        Ok(ConsensusParameters(params))
    }

    #[graphql(complexity = "QUERY_COSTS.storage_read + child_complexity")]
    async fn state_transition_bytecode_by_version(
        &self,
        ctx: &Context<'_>,
        version: StateTransitionBytecodeVersion,
    ) -> async_graphql::Result<Option<StateTransitionBytecode>> {
        let query = ctx.read_view()?;
        query
            .state_transition_bytecode_root(version)
            .into_api_result()
    }

    #[graphql(complexity = "QUERY_COSTS.storage_read + child_complexity")]
    async fn state_transition_bytecode_by_root(
        &self,
        root: HexString,
    ) -> async_graphql::Result<StateTransitionBytecode> {
        StateTransitionBytecode::try_from(root)
    }
}

pub struct StateTransitionBytecode {
    root: fuel_types::Bytes32,
}

#[Object]
impl StateTransitionBytecode {
    async fn root(&self) -> HexString {
        HexString(self.root.to_vec())
    }

    async fn bytecode(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<UploadedBytecode> {
        let query = ctx.read_view()?;
        query
            .state_transition_bytecode(self.root)
            .map(UploadedBytecode::from)
            .map_err(async_graphql::Error::from)
    }
}

impl From<fuel_types::Bytes32> for StateTransitionBytecode {
    fn from(root: fuel_types::Bytes32) -> Self {
        Self { root }
    }
}

impl TryFrom<HexString> for StateTransitionBytecode {
    type Error = async_graphql::Error;

    fn try_from(root: HexString) -> Result<Self, Self::Error> {
        let root = root.0.as_slice().try_into()?;
        Ok(Self { root })
    }
}

#[derive(SimpleObject)]
pub struct UploadedBytecode {
    /// Combined bytecode of all uploaded subsections.
    bytecode: HexString,
    /// Number of uploaded subsections (if incomplete).
    uploaded_subsections_number: Option<u16>,
    /// Indicates if the bytecode upload is complete.
    completed: bool,
}

impl From<StorageUploadedBytecode> for UploadedBytecode {
    fn from(value: fuel_core_types::fuel_vm::UploadedBytecode) -> Self {
        match value {
            StorageUploadedBytecode::Uncompleted {
                bytecode,
                uploaded_subsections_number,
            } => Self {
                bytecode: HexString(bytecode),
                uploaded_subsections_number: Some(uploaded_subsections_number),
                completed: false,
            },
            StorageUploadedBytecode::Completed(bytecode) => Self {
                bytecode: HexString(bytecode),
                uploaded_subsections_number: None,
                completed: true,
            },
        }
    }
}
