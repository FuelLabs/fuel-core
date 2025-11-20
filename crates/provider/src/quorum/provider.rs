use crate::{
    Quorum,
    quorum::transport::{
        QuorumTransport,
        WeightedTransport,
    },
};
use alloy_provider::{
    Provider,
    RootProvider,
    network::Ethereum,
};
use alloy_rpc_client::RpcClient;
use alloy_transport::IntoBoxTransport;
#[cfg(feature = "test-helpers")]
use alloy_transport::mock::{
    Asserter,
    MockTransport,
};
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
            .with_quorum(quorum)
            .with_transports(transports)
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
    pub fn with_quorum(mut self, quorum: Quorum) -> Self {
        self.quorum = quorum;
        self
    }

    pub fn with_urls(mut self, urls: Vec<Url>) -> Self {
        self.transports.extend(urls.into_iter().map(|url| {
            WeightedTransport::new(
                alloy_transport_http::Http::new(url).into_box_transport(),
            )
        }));
        self
    }

    #[cfg(feature = "test-helpers")]
    pub fn with_mocked_transport(mut self, asserter: Asserter) -> Self {
        self.transports.push(WeightedTransport::new(
            MockTransport::new(asserter).into_box_transport(),
        ));
        self
    }

    pub fn with_transports(mut self, transports: Vec<WeightedTransport>) -> Self {
        self.transports.extend(transports);
        self
    }

    pub fn build(self) -> QuorumProvider {
        let transport = QuorumTransport::builder()
            .with_quorum(self.quorum)
            .with_transports(self.transports)
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
        Quorum,
        QuorumProvider,
        quorum::transport::WeightedTransport,
    };
    use alloy_json_rpc::ErrorPayload;
    use alloy_provider::Provider;
    use alloy_transport::{
        IntoBoxTransport,
        mock::{
            Asserter,
            MockTransport,
        },
    };
    use std::num::{
        NonZeroU64,
        NonZeroUsize,
    };

    #[tokio::test]
    async fn test_quorum_provider_majority_success() {
        const EXPECTED_BLOCK_NUMBER: u64 = 42u64;

        let transports = vec![
            WeightedTransport::new(
                MockTransport::new(successful_asserter_u64(&EXPECTED_BLOCK_NUMBER))
                    .into_box_transport(),
            ),
            WeightedTransport::new(
                MockTransport::new(successful_asserter_u64(&EXPECTED_BLOCK_NUMBER))
                    .into_box_transport(),
            ),
            WeightedTransport::new(
                MockTransport::new(successful_asserter_u64(&EXPECTED_BLOCK_NUMBER))
                    .into_box_transport(),
            ),
        ];

        let provider = QuorumProvider::builder()
            .with_quorum(Quorum::Majority)
            .with_transports(transports)
            .build();

        let block_number = provider.get_block_number().await.unwrap();

        assert_eq!(block_number, 42);
    }

    #[tokio::test]
    async fn test_quorum_provider_all_success() {
        const EXPECTED_BLOCK_NUMBER: u64 = 100u64;

        let transports = vec![
            WeightedTransport::new(
                MockTransport::new(successful_asserter_u64(&EXPECTED_BLOCK_NUMBER))
                    .into_box_transport(),
            ),
            WeightedTransport::new(
                MockTransport::new(successful_asserter_u64(&EXPECTED_BLOCK_NUMBER))
                    .into_box_transport(),
            ),
            WeightedTransport::new(
                MockTransport::new(successful_asserter_u64(&EXPECTED_BLOCK_NUMBER))
                    .into_box_transport(),
            ),
        ];

        let provider = QuorumProvider::builder()
            .with_quorum(Quorum::All)
            .with_transports(transports)
            .build();

        let block_number = provider.get_block_number().await.unwrap();
        assert_eq!(block_number, EXPECTED_BLOCK_NUMBER);
    }

    #[tokio::test]
    async fn test_quorum_provider_weight_success() {
        const EXPECTED_BLOCK_NUMBER: u64 = 88u64;

        let transports = vec![
            WeightedTransport::with_weight(
                MockTransport::new(successful_asserter_u64(&EXPECTED_BLOCK_NUMBER))
                    .into_box_transport(),
                2,
            ),
            WeightedTransport::with_weight(
                MockTransport::new(successful_asserter_u64(&EXPECTED_BLOCK_NUMBER))
                    .into_box_transport(),
                3,
            ),
            WeightedTransport::with_weight(
                MockTransport::new(failing_asserter()).into_box_transport(),
                1,
            ),
        ];

        let provider = QuorumProvider::builder()
            .with_quorum(Quorum::Weight(NonZeroU64::new(2).unwrap()))
            .with_transports(transports)
            .build();

        let block_number = provider.get_block_number().await.unwrap();
        assert_eq!(block_number, EXPECTED_BLOCK_NUMBER);
    }

    #[tokio::test]
    async fn test_quorum_provider_count_success() {
        const EXPECTED_BLOCK_NUMBER: u64 = 7;

        let transports = vec![
            WeightedTransport::with_weight(
                MockTransport::new(successful_asserter_u64(&EXPECTED_BLOCK_NUMBER))
                    .into_box_transport(),
                10,
            ),
            WeightedTransport::with_weight(
                MockTransport::new(successful_asserter_u64(&EXPECTED_BLOCK_NUMBER))
                    .into_box_transport(),
                1,
            ),
            WeightedTransport::with_weight(
                MockTransport::new(failing_asserter()).into_box_transport(),
                1,
            ),
        ];

        let provider = QuorumProvider::builder()
            .with_quorum(Quorum::ProviderCount(NonZeroUsize::new(2).unwrap()))
            .with_transports(transports)
            .build();

        let block_number = provider.get_block_number().await.unwrap();
        assert_eq!(block_number, EXPECTED_BLOCK_NUMBER);
    }

    #[tokio::test]
    async fn test_quorum_provider_all_with_one_failure() {
        let value1 = 1u64;
        let value2 = 2u64;

        let transports = vec![
            WeightedTransport::new(
                MockTransport::new(successful_asserter_u64(&value1)).into_box_transport(),
            ),
            WeightedTransport::new(
                MockTransport::new(successful_asserter_u64(&value2)).into_box_transport(),
            ),
            WeightedTransport::new(
                MockTransport::new(successful_asserter_u64(&value2)).into_box_transport(),
            ),
        ];

        let provider = QuorumProvider::builder()
            .with_quorum(Quorum::All)
            .with_transports(transports)
            .build();

        let result = provider.get_block_number().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_quorum_provider_majority_with_one_failure() {
        const EXPECTED_BLOCK_NUMBER: u64 = 43;

        let transports = vec![
            WeightedTransport::new(
                MockTransport::new(successful_asserter_u64(&EXPECTED_BLOCK_NUMBER))
                    .into_box_transport(),
            ),
            WeightedTransport::new(
                MockTransport::new(successful_asserter_u64(&EXPECTED_BLOCK_NUMBER))
                    .into_box_transport(),
            ),
            WeightedTransport::new(
                MockTransport::new(failing_asserter()).into_box_transport(),
            ),
        ];

        let provider = QuorumProvider::builder()
            .with_quorum(Quorum::Majority)
            .with_transports(transports)
            .build();

        let block_number = provider.get_block_number().await;
        assert_eq!(block_number.unwrap(), EXPECTED_BLOCK_NUMBER);
    }

    #[tokio::test]
    async fn test_quorum_provider_majority_with_one_different_value() {
        const EXPECTED_BLOCK_NUMBER: u64 = 43;

        let transports = vec![
            WeightedTransport::new(
                MockTransport::new(successful_asserter_u64(&EXPECTED_BLOCK_NUMBER))
                    .into_box_transport(),
            ),
            WeightedTransport::new(
                MockTransport::new(successful_asserter_u64(&EXPECTED_BLOCK_NUMBER))
                    .into_box_transport(),
            ),
            WeightedTransport::new(
                MockTransport::new(successful_asserter_u64(&1)).into_box_transport(),
            ),
        ];

        let provider = QuorumProvider::builder()
            .with_quorum(Quorum::Majority)
            .with_transports(transports)
            .build();

        let block_number = provider.get_block_number().await;
        assert_eq!(block_number.unwrap(), EXPECTED_BLOCK_NUMBER);
    }

    fn successful_asserter_u64(response: &u64) -> Asserter {
        let asserter = Asserter::new();
        asserter.push_success(response);
        asserter
    }

    fn failing_asserter() -> Asserter {
        let asserter = Asserter::new();
        asserter.push_failure(ErrorPayload::internal_error());
        asserter
    }
}
