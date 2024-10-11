use crate::{
    fuel_core_graphql_api::validation_extension::visitor::{
        visit,
        RuleError,
        VisitorContext,
    },
    graphql_api::validation_extension::recursion_finder::RecursionFinder,
};
use async_graphql::{
    extensions::{
        Extension,
        ExtensionContext,
        ExtensionFactory,
        NextParseQuery,
        NextValidation,
    },
    parser::types::ExecutableDocument,
    ServerError,
    ServerResult,
    ValidationResult,
    Variables,
};
use std::sync::{
    Arc,
    Mutex,
};

mod recursion_finder;
mod visitor;

/// The extension validates the queries.
pub(crate) struct ValidationExtension {
    recursion_limit: usize,
}

impl ValidationExtension {
    pub fn new(recursion_limit: usize) -> Self {
        Self { recursion_limit }
    }
}

impl ExtensionFactory for ValidationExtension {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(ValidationInner::new(self.recursion_limit))
    }
}

struct ValidationInner {
    recursion_limit: usize,
    errors: Mutex<Option<Vec<RuleError>>>,
}

impl ValidationInner {
    fn new(recursion_limit: usize) -> Self {
        Self {
            recursion_limit,
            errors: Default::default(),
        }
    }
}

#[async_trait::async_trait]
impl Extension for ValidationInner {
    async fn parse_query(
        &self,
        ctx: &ExtensionContext<'_>,
        query: &str,
        variables: &Variables,
        next: NextParseQuery<'_>,
    ) -> ServerResult<ExecutableDocument> {
        let result = next.run(ctx, query, variables).await?;
        let registry = &ctx.schema_env.registry;
        let mut visitor = VisitorContext::new(registry, &result, Some(variables));
        visit(
            &mut RecursionFinder::new(self.recursion_limit),
            &mut visitor,
            &result,
        );

        let errors = visitor.errors;

        if !errors.is_empty() {
            let mut store = self
                .errors
                .lock()
                .expect("Only one instance owns `ValidationInner`; qed");
            *store = Some(errors);
        }

        Ok(result)
    }

    async fn validation(
        &self,
        ctx: &ExtensionContext<'_>,
        next: NextValidation<'_>,
    ) -> async_graphql::Result<ValidationResult, Vec<ServerError>> {
        if let Some(errors) = self
            .errors
            .lock()
            .expect("Only one instance owns `ValidationInner`; qed")
            .take()
        {
            return Err(errors.into_iter().map(Into::into).collect())
        }

        next.run(ctx).await
    }
}
