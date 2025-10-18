use crate::quorum::transport::{QuorumTransportBuilder, WeightedTransport};
use crate::Quorum;
use alloy_provider::network::Ethereum;
use alloy_provider::{Provider, RootProvider};
use alloy_rpc_client::RpcClient;
use alloy_transport::IntoBoxTransport;
use url::Url;

pub struct QuorumProvider {
    inner: RootProvider,
}

impl QuorumProvider {
    pub fn new(quorum: Quorum, urls: Vec<Url>) -> Self {
        let transports: Vec<_> = urls.into_iter()
            .map(|url| WeightedTransport::new(alloy_transport_http::Http::new(url).into_box_transport())).collect();
        let transport = QuorumTransportBuilder::default().quorum(quorum).add_transports(transports).build();
        let inner = alloy_provider::RootProvider::new(RpcClient::new(transport, false));
        Self { inner }
    }
}

impl Provider for QuorumProvider {
    fn root(&self) -> &RootProvider<Ethereum> {
        &self.inner
    }
}