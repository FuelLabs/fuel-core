use prometheus_client::{
    encoding::text::encode,
    registry::Registry,
};
use std::{sync::Arc, any};
use anyhow::*;
use axum::{
    body::Body,
    http::header::CONTENT_TYPE,
    response::{
        IntoResponse,
        Response,
    },
};

pub struct ServiceBuilder {
    p2p_metrics: Option<Arc<Registry>>,
    txpool_metrics: Option<Arc<Registry>>,
    db_metrics: Option<Arc<Registry>>,
}

impl ServiceBuilder {
    pub fn new() -> Self {
        ServiceBuilder { 
            p2p_metrics: None, 
            txpool_metrics: None, 
            db_metrics: None 
        }
    }

    pub fn set_p2p_registry(&mut self, p2p_registry: Arc<Registry>) {
        self.p2p_metrics = Some(p2p_registry);
    }

    pub fn set_txpool_metrics(&mut self, txpool_metrics: Arc<Registry>) {
        self.txpool_metrics = Some(txpool_metrics);
    }

    pub fn set_db_metrics(&mut self, db_metrics: Arc<Registry>) {
        self.db_metrics = Some(db_metrics);
    }

    pub fn build(self) -> Result<Service, Error> {
        if self.db_metrics.is_none() || self.txpool_metrics.is_none() || self.p2p_metrics.is_none() {
            return Err(anyhow!("One or more context items not set"));
        }

        Ok(Service {
            db_metrics: self.db_metrics.unwrap(),
            txpool_metrics: self.txpool_metrics.unwrap(),
            p2p_metrics: self.p2p_metrics.unwrap(),
        })
    }
}

pub struct Service {
    p2p_metrics: Arc<Registry>,
    txpool_metrics: Arc<Registry>,
    db_metrics: Arc<Registry>,
}

impl Service {
    pub fn encode_p2p_metrics_response(self) -> impl IntoResponse {
        let mut encoded = vec![];
        encode(&mut encoded, &self.p2p_metrics).unwrap();
        Response::builder()
            .status(200)
            .body(Body::from(encoded))
            .unwrap()
    }
}