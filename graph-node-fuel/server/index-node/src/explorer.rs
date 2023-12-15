//! Functionality to support the explorer in the hosted service. Everything
//! in this file is private API and experimental and subject to change at
//! any time
use graph::prelude::r;
use http::{Response, StatusCode};
use hyper::header::{
    ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN,
    CONTENT_TYPE,
};
use hyper::Body;
use std::{sync::Arc, time::Instant};

use graph::{
    components::{
        server::{index_node::VersionInfo, query::GraphQLServerError},
        store::StatusStore,
    },
    data::subgraph::status,
    object,
    prelude::{serde_json, warn, Logger, ENV_VARS},
    util::timed_cache::TimedCache,
};

// Do not implement `Clone` for this; the IndexNode service puts the `Explorer`
// behind an `Arc` so we don't have to put each `Cache` into an `Arc`
//
// We cache responses for a fixed amount of time with the time given by
// `GRAPH_EXPLORER_TTL`
#[derive(Debug)]
pub struct Explorer<S> {
    store: Arc<S>,
    versions: TimedCache<String, r::Value>,
    version_infos: TimedCache<String, VersionInfo>,
    entity_counts: TimedCache<String, r::Value>,
}

impl<S> Explorer<S>
where
    S: StatusStore,
{
    pub fn new(store: Arc<S>) -> Self {
        Self {
            store,
            versions: TimedCache::new(ENV_VARS.explorer_ttl),
            version_infos: TimedCache::new(ENV_VARS.explorer_ttl),
            entity_counts: TimedCache::new(ENV_VARS.explorer_ttl),
        }
    }

    pub fn handle(
        &self,
        logger: &Logger,
        req: &[&str],
    ) -> Result<Response<Body>, GraphQLServerError> {
        match req {
            ["subgraph-versions", subgraph_id] => self.handle_subgraph_versions(subgraph_id),
            ["subgraph-version", version] => self.handle_subgraph_version(version),
            ["subgraph-repo", version] => self.handle_subgraph_repo(version),
            ["entity-count", deployment] => self.handle_entity_count(logger, deployment),
            ["subgraphs-for-deployment", deployment_hash] => {
                self.handle_subgraphs_for_deployment(deployment_hash)
            }
            _ => handle_not_found(),
        }
    }

    fn handle_subgraph_versions(
        &self,
        subgraph_id: &str,
    ) -> Result<Response<Body>, GraphQLServerError> {
        if let Some(value) = self.versions.get(subgraph_id) {
            return Ok(as_http_response(value.as_ref()));
        }

        let (current, pending) = self.store.versions_for_subgraph_id(subgraph_id)?;

        let value = object! {
            currentVersion: current,
            pendingVersion: pending
        };

        let resp = as_http_response(&value);
        self.versions.set(subgraph_id.to_string(), Arc::new(value));
        Ok(resp)
    }

    fn handle_subgraph_version(&self, version: &str) -> Result<Response<Body>, GraphQLServerError> {
        let vi = self.version_info(version)?;

        let latest_ethereum_block_number = vi.latest_ethereum_block_number;
        let total_ethereum_blocks_count = vi.total_ethereum_blocks_count;
        let value = object! {
            createdAt: vi.created_at.as_str(),
            deploymentId: vi.deployment_id.as_str(),
            latestEthereumBlockNumber: latest_ethereum_block_number,
            totalEthereumBlocksCount: total_ethereum_blocks_count,
            synced: vi.synced,
            failed: vi.failed,
            description: vi.description.as_deref(),
            repository: vi.repository.as_deref(),
            schema: vi.schema.document_string(),
            network: vi.network.as_str()
        };
        Ok(as_http_response(&value))
    }

    fn handle_subgraph_repo(&self, version: &str) -> Result<Response<Body>, GraphQLServerError> {
        let vi = self.version_info(version)?;

        let value = object! {
            createdAt: vi.created_at.as_str(),
            deploymentId: vi.deployment_id.as_str(),
            repository: vi.repository.as_deref()
        };
        Ok(as_http_response(&value))
    }

    fn handle_entity_count(
        &self,
        logger: &Logger,
        deployment: &str,
    ) -> Result<Response<Body>, GraphQLServerError> {
        let start = Instant::now();
        let count = self.entity_counts.get(deployment);
        if start.elapsed() > ENV_VARS.explorer_lock_threshold {
            let action = match count {
                Some(_) => "cache_hit",
                None => "cache_miss",
            };
            warn!(logger, "Getting entity_count takes too long";
                       "action" => action,
                       "deployment" => deployment,
                       "time_ms" => start.elapsed().as_millis());
        }

        if let Some(value) = count {
            return Ok(as_http_response(value.as_ref()));
        }

        let start = Instant::now();
        let infos = self
            .store
            .status(status::Filter::Deployments(vec![deployment.to_string()]))?;
        if start.elapsed() > ENV_VARS.explorer_query_threshold {
            warn!(logger, "Getting entity_count takes too long";
            "action" => "query_status",
            "deployment" => deployment,
            "time_ms" => start.elapsed().as_millis());
        }
        let info = match infos.first() {
            Some(info) => info,
            None => {
                return handle_not_found();
            }
        };

        let value = object! {
            entityCount: info.entity_count as i32
        };
        let start = Instant::now();
        let resp = as_http_response(&value);
        if start.elapsed() > ENV_VARS.explorer_lock_threshold {
            warn!(logger, "Getting entity_count takes too long";
            "action" => "as_http_response",
            "deployment" => deployment,
            "time_ms" => start.elapsed().as_millis());
        }
        let start = Instant::now();
        self.entity_counts
            .set(deployment.to_string(), Arc::new(value));
        if start.elapsed() > ENV_VARS.explorer_lock_threshold {
            warn!(logger, "Getting entity_count takes too long";
                "action" => "cache_set",
                "deployment" => deployment,
                "time_ms" => start.elapsed().as_millis());
        }
        Ok(resp)
    }

    fn version_info(&self, version: &str) -> Result<Arc<VersionInfo>, GraphQLServerError> {
        match self.version_infos.get(version) {
            Some(vi) => Ok(vi),
            None => {
                let vi = Arc::new(self.store.version_info(version)?);
                self.version_infos.set(version.to_string(), vi.clone());
                Ok(vi)
            }
        }
    }

    fn handle_subgraphs_for_deployment(
        &self,
        deployment_hash: &str,
    ) -> Result<Response<Body>, GraphQLServerError> {
        let name_version_pairs: Vec<r::Value> = self
            .store
            .subgraphs_for_deployment_hash(deployment_hash)?
            .into_iter()
            .map(|(name, version)| {
                object! {
                    name: name,
                    version: version
                }
            })
            .collect();
        let payload = r::Value::List(name_version_pairs);
        Ok(as_http_response(&payload))
    }
}

fn handle_not_found() -> Result<Response<Body>, GraphQLServerError> {
    Ok(Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(CONTENT_TYPE, "text/plain")
        .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(Body::from("Not found\n"))
        .unwrap())
}

fn as_http_response(value: &r::Value) -> http::Response<Body> {
    let status_code = http::StatusCode::OK;
    let json = serde_json::to_string(&value).expect("Failed to serialize response to JSON");
    http::Response::builder()
        .status(status_code)
        .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(ACCESS_CONTROL_ALLOW_HEADERS, "Content-Type, User-Agent")
        .header(ACCESS_CONTROL_ALLOW_METHODS, "GET, OPTIONS, POST")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json))
        .unwrap()
}
