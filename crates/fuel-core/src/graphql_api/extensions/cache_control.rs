use async_graphql::{
    Request,
    Response,
    ServerResult,
    Value,
    extensions::{
        Extension,
        ExtensionContext,
        ExtensionFactory,
        NextPrepareRequest,
        NextRequest,
        NextResolve,
        ResolveInfo,
    },
    registry::MetaType,
};
use std::sync::{
    Arc,
    atomic::{
        AtomicBool,
        Ordering,
    },
};

/// Require an explicit cache policy for root fields and nested object types.
/// async-graphql otherwise treats unannotated fields as neutral, allowing a
/// mutable selection to inherit the cache lifetime of an unrelated field.
pub(crate) struct CacheControlExtension;

impl ExtensionFactory for CacheControlExtension {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(CacheControlInner {
            veto: AtomicBool::new(false),
        })
    }
}

struct CacheControlInner {
    veto: AtomicBool,
}

#[async_trait::async_trait]
impl Extension for CacheControlInner {
    async fn prepare_request(
        &self,
        ctx: &ExtensionContext<'_>,
        request: Request,
        next: NextPrepareRequest<'_>,
    ) -> ServerResult<Request> {
        // Request extensions can impose conditions on current node state, such
        // as required_fuel_block_height. Keep those requests at the origin.
        if !request.extensions.is_empty() {
            self.veto.store(true, Ordering::Relaxed);
        }
        next.run(ctx, request).await
    }

    async fn request(
        &self,
        ctx: &ExtensionContext<'_>,
        next: NextRequest<'_>,
    ) -> Response {
        let mut response = next.run(ctx).await;
        if self.veto.load(Ordering::Relaxed) || !response.is_ok() {
            response.cache_control.max_age = -1;
        }
        response
    }

    async fn resolve(
        &self,
        ctx: &ExtensionContext<'_>,
        info: ResolveInfo<'_>,
        next: NextResolve<'_>,
    ) -> ServerResult<Option<Value>> {
        // async-graphql resolves __typename before invoking resolve extensions,
        // including on unions, so it does not need a registry cache policy here.
        let eligible = match ctx.schema_env.registry.types.get(info.parent_type) {
            Some(MetaType::Object {
                fields,
                cache_control,
                ..
            }) => fields.get(info.name).is_some_and(|field| {
                cache_control.max_age >= 0
                    && field.cache_control.max_age >= 0
                    && (field.cache_control.max_age > 0 || cache_control.max_age > 0)
            }),
            _ => false,
        };
        if !eligible || info.is_for_introspection {
            self.veto.store(true, Ordering::Relaxed);
        }

        let is_root = info.path_node.parent.is_none();
        let result = next.run(ctx, info).await;
        // A missing historical object can become available later. Optional
        // fields within an existing immutable snapshot may legitimately be null.
        if is_root && !matches!(&result, Ok(Some(value)) if value != &Value::Null) {
            self.veto.store(true, Ordering::Relaxed);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql::{
        EmptyMutation,
        EmptySubscription,
        Object,
        Schema,
        SimpleObject,
    };

    #[derive(SimpleObject)]
    struct Mutable {
        value: u32,
    }

    struct Historical;

    #[Object(cache_control(max_age = 60))]
    impl Historical {
        async fn value(&self) -> u32 {
            1
        }

        async fn optional(&self) -> Option<u32> {
            None
        }

        async fn nested(&self) -> Mutable {
            Mutable { value: 2 }
        }

        #[graphql(cache_control(no_cache))]
        async fn live(&self) -> u32 {
            3
        }

        #[graphql(cache_control(max_age = 30, private))]
        async fn private(&self) -> u32 {
            4
        }

        async fn failed(&self) -> async_graphql::Result<u32> {
            Err("storage unavailable".into())
        }
    }

    struct Query;

    #[Object]
    impl Query {
        #[graphql(cache_control(max_age = 60))]
        async fn historical(&self, exists: Option<bool>) -> Option<Historical> {
            exists.unwrap_or(true).then_some(Historical)
        }

        async fn mutable(&self) -> u32 {
            5
        }
    }

    #[tokio::test]
    async fn cache_control_requires_eligible_selections_and_results() {
        let schema = Schema::build(Query, EmptyMutation, EmptySubscription)
            .extension(CacheControlExtension)
            .finish();

        for (query, age) in [
            ("{ historical { value optional } }", 60),
            ("{ __typename historical { __typename value } }", 60),
            ("{ historical { value } mutable }", -1),
            ("{ __typename historical { __typename value } mutable }", -1),
            (
                "{ historical { value } __schema { queryType { name } } }",
                -1,
            ),
            ("{ historical { nested { value } } }", -1),
            ("{ historical { live } }", -1),
            ("{ historical(exists: false) { value } }", -1),
            ("{ historical { failed } }", -1),
            ("{ unknown }", -1),
            // A previous request's veto must not affect the next request.
            ("{ historical { value } }", 60),
        ] {
            let response = schema.execute(query).await;
            assert_eq!(response.cache_control.max_age, age, "{query}");
        }

        let private = schema.execute("{ historical { value private } }").await;
        assert_eq!(private.cache_control.max_age, 30);
        assert!(!private.cache_control.public);

        let mut request = Request::new("{ historical { value } }");
        request
            .extensions
            .insert("required_fuel_block_height".into(), 0.into());
        assert_eq!(schema.execute(request).await.cache_control.max_age, -1);
    }
}
