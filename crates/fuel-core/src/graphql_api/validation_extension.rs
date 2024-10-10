use async_graphql::{
    extensions::{
        Extension,
        ExtensionContext,
        ExtensionFactory,
        NextResolve,
        ResolveInfo,
    },
    Pos,
    ServerError,
    ServerResult,
    Value,
};
use std::{
    collections::{
        hash_map::Entry,
        HashMap,
    },
    sync::Arc,
};

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
    visited: parking_lot::Mutex<HashMap<String, usize>>,
}

impl ValidationInner {
    fn new(recursion_limit: usize) -> Self {
        Self {
            recursion_limit,
            visited: parking_lot::Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl Extension for ValidationInner {
    async fn resolve(
        &self,
        ctx: &ExtensionContext<'_>,
        info: ResolveInfo<'_>,
        next: NextResolve<'_>,
    ) -> ServerResult<Option<Value>> {
        let name = info.return_type.to_string();
        {
            let mut lock = self.visited.lock();
            let old = lock.entry(name.clone()).or_default();
            *old = old.saturating_add(1);

            if *old > self.recursion_limit {
                let (line, column) = (line!(), column!());
                return Err(ServerError::new(
                    format!("Recursion detected for field `{}`", name),
                    Some(Pos {
                        line: line as usize,
                        column: column as usize,
                    }),
                ));
            }
        }
        let result = next.run(ctx, info).await;
        let mut lock = self.visited.lock();
        let old = lock.entry(name);
        match old {
            Entry::Occupied(entry) => {
                if entry.get() == &1 {
                    entry.remove();
                } else {
                    let value = entry.into_mut();
                    *value = value.saturating_sub(1);
                }
            }
            Entry::Vacant(_) => {
                // Shouldn't be possible.
            }
        }

        result
    }
}
