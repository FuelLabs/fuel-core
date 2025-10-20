use crate::{
    quorum::transport::{
        QuorumTransport,
        WeightedTransport,
    },
    Quorum,
};
use alloy_provider::{
    network::Ethereum,
    Provider,
    RootProvider,
};
use alloy_rpc_client::RpcClient;
use alloy_transport::mock::{Asserter, MockTransport};
use alloy_transport::IntoBoxTransport;
use url::Url;

pub struct QuorumProvider {
    inner: RootProvider,
}

impl QuorumProvider {
    pub fn new(quorum: Quorum, urls: Vec<Url>) -> Self {
        let transports: Vec<_> = urls
            .into_iter()
            .map(|url| {
                WeightedTransport::new(
                    alloy_transport_http::Http::new(url).into_box_transport(),
                )
            })
            .collect();
        let transport = QuorumTransport::builder()
            .quorum(quorum)
            .add_transports(transports)
            .build();
        let inner = RootProvider::new(RpcClient::new(transport, false));
        Self { inner }
    }

    pub fn builder() -> QuorumProviderBuilder {
        QuorumProviderBuilder::default()
    }
}

#[derive(Default)]
pub struct QuorumProviderBuilder {
    quorum: Quorum,
    transports: Vec<WeightedTransport>,
}

impl QuorumProviderBuilder {
    pub fn quorum(mut self, quorum: Quorum) -> Self {
        self.quorum = quorum;
        self
    }

    pub fn urls(mut self, urls: Vec<Url>) -> Self {
        self.transports.extend(urls.into_iter().map(|url| {
            WeightedTransport::new(
                alloy_transport_http::Http::new(url).into_box_transport(),
            )
        }));
        self
    }

    pub fn add_mocked_transport(mut self, asserter: Asserter) -> Self {
        self.transports.push(WeightedTransport::new(MockTransport::new(asserter).into_box_transport()));
        self
    }

    pub fn transports(mut self, transports: Vec<WeightedTransport>) -> Self {
        self.transports.extend(transports);
        self
    }

    pub fn build(self) -> QuorumProvider {
        let transport = QuorumTransport::builder()
            .quorum(self.quorum)
            .add_transports(self.transports)
            .build();

        let inner = RootProvider::new(RpcClient::new(transport, false));
        QuorumProvider { inner }
    }
}

impl Provider for QuorumProvider {
    fn root(&self) -> &RootProvider<Ethereum> {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        quorum::transport::WeightedTransport,
        Quorum,
        QuorumProvider,
    };
    use alloy_json_rpc::ErrorPayload;
    use alloy_primitives::private::serde::Serialize;
    use alloy_provider::Provider;
    use alloy_transport::{
        mock::{
            Asserter,
            MockTransport,
        },
        IntoBoxTransport,
    };

    #[tokio::test]
    async fn test_quorum_provider_majority_success() {
        const EXPECTED_BLOCK_NUMBER: u64 = 42u64;

        let transports = vec![
            WeightedTransport::new(
                MockTransport::new(successful_asserter(&EXPECTED_BLOCK_NUMBER))
                    .into_box_transport(),
            ),
            WeightedTransport::new(
                MockTransport::new(successful_asserter(&EXPECTED_BLOCK_NUMBER))
                    .into_box_transport(),
            ),
            WeightedTransport::new(
                MockTransport::new(successful_asserter(&EXPECTED_BLOCK_NUMBER))
                    .into_box_transport(),
            ),
        ];

        let provider = QuorumProvider::builder()
            .quorum(Quorum::Majority)
            .transports(transports)
            .build();

        let block_number = provider.get_block_number().await.unwrap();

        assert_eq!(block_number, 42);
    }

    #[tokio::test]
    async fn test_quorum_provider_all_success() {
        const EXPECTED_BLOCK_NUMBER: u64 = 100u64;

        let transports = vec![
            WeightedTransport::new(
                MockTransport::new(successful_asserter(&EXPECTED_BLOCK_NUMBER))
                    .into_box_transport(),
            ),
            WeightedTransport::new(
                MockTransport::new(successful_asserter(&EXPECTED_BLOCK_NUMBER))
                    .into_box_transport(),
            ),
            WeightedTransport::new(
                MockTransport::new(successful_asserter(&EXPECTED_BLOCK_NUMBER))
                    .into_box_transport(),
            ),
        ];

        let provider = QuorumProvider::builder()
            .quorum(Quorum::All)
            .transports(transports)
            .build();

        let block_number = provider.get_block_number().await.unwrap();
        assert_eq!(block_number, EXPECTED_BLOCK_NUMBER);
    }

    #[tokio::test]
    async fn test_quorum_provider_weight_success() {
        const EXPECTED_BLOCK_NUMBER: u64 = 88u64;

        let transports = vec![
            WeightedTransport::with_weight(
                MockTransport::new(successful_asserter(&EXPECTED_BLOCK_NUMBER))
                    .into_box_transport(),
                2,
            ),
            WeightedTransport::with_weight(
                MockTransport::new(successful_asserter(&EXPECTED_BLOCK_NUMBER))
                    .into_box_transport(),
                3,
            ),
            WeightedTransport::with_weight(
                MockTransport::new(failed_asserter()).into_box_transport(),
                2,
            ),
        ];

        let provider = QuorumProvider::builder()
            .quorum(Quorum::Weight(5))
            .transports(transports)
            .build();

        let block_number = provider.get_block_number().await.unwrap();
        assert_eq!(block_number, EXPECTED_BLOCK_NUMBER);
    }

    #[tokio::test]
    async fn test_quorum_provider_count_success() {
        const EXPECTED_BLOCK_NUMBER: u64 = 7;

        let transports = vec![
            WeightedTransport::with_weight(
                MockTransport::new(successful_asserter(&EXPECTED_BLOCK_NUMBER))
                    .into_box_transport(),
                10,
            ),
            WeightedTransport::with_weight(
                MockTransport::new(successful_asserter(&EXPECTED_BLOCK_NUMBER))
                    .into_box_transport(),
                1,
            ),
            WeightedTransport::with_weight(
                MockTransport::new(failed_asserter()).into_box_transport(),
                1,
            ),
        ];

        let provider = QuorumProvider::builder()
            .quorum(Quorum::ProviderCount(2))
            .transports(transports)
            .build();

        let block_number = provider.get_block_number().await.unwrap();
        assert_eq!(block_number, EXPECTED_BLOCK_NUMBER);
    }

    #[tokio::test]
    async fn test_quorum_provider_failure() {
        let value1 = 1u64;
        let value2 = 2u64;

        let transports = vec![
            WeightedTransport::new(
                MockTransport::new(successful_asserter(&value1)).into_box_transport(),
            ),
            WeightedTransport::new(
                MockTransport::new(successful_asserter(&value2)).into_box_transport(),
            ),
            WeightedTransport::new(
                MockTransport::new(successful_asserter(&value2)).into_box_transport(),
            ),
        ];

        let provider = QuorumProvider::builder()
            .quorum(Quorum::All)
            .transports(transports)
            .build();

        let result = provider.get_block_number().await;
        assert!(result.is_err());
    }

    fn successful_asserter<R: Serialize>(response: &R) -> Asserter {
        let asserter = Asserter::new();
        asserter.push_success(response);
        asserter
    }

    fn failed_asserter() -> Asserter {
        let asserter = Asserter::new();
        asserter.push_failure(ErrorPayload::internal_error());
        asserter
    }
}
