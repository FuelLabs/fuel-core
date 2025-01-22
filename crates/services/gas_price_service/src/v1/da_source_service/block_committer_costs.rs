#![allow(clippy::arithmetic_side_effects)]

use crate::v1::da_source_service::{
    service::{
        DaBlockCostsSource,
        Result as DaBlockCostsResult,
    },
    DaBlockCosts,
};
use anyhow::anyhow;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_types::BlockHeight,
};
use serde::{
    Deserialize,
    Serialize,
};
use std::ops::Deref;

#[async_trait::async_trait]
pub trait BlockCommitterApi: Send + Sync {
    /// Used to get the costs for a specific seqno
    async fn get_costs_by_l2_block_number(
        &self,
        l2_block_number: u32,
    ) -> DaBlockCostsResult<Vec<RawDaBlockCosts>>;
}

/// This struct is used to denote the block committer da block costs source
/// which receives data from the block committer (only http api for now)
pub struct BlockCommitterDaBlockCosts<BlockCommitter> {
    client: BlockCommitter,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq)]
pub struct RawDaBlockCosts {
    pub id: u32,
    /// The beginning of the range of blocks that the costs apply to
    pub start_height: u32,
    /// The end of the range of blocks that the costs apply to
    pub end_height: u32,
    /// The DA block height of the last transaction for the range of blocks
    pub da_block_height: DaBlockHeight,
    /// cost of posting this bundle (wei)
    pub cost: u128,
    /// size of this bundle (bytes)
    pub size: u32,
}

impl From<&RawDaBlockCosts> for DaBlockCosts {
    fn from(raw_da_block_costs: &RawDaBlockCosts) -> Self {
        let RawDaBlockCosts {
            start_height,
            end_height,
            cost: cost_wei,
            size: size_bytes,
            id: bundle_id,
            ..
        } = *raw_da_block_costs;
        DaBlockCosts {
            bundle_id,
            // construct a vec of l2 blocks from the start_height to the end_height
            l2_blocks: start_height..=end_height,
            bundle_size_bytes: size_bytes,
            blob_cost_wei: cost_wei,
        }
    }
}

impl From<RawDaBlockCosts> for DaBlockCosts {
    fn from(value: RawDaBlockCosts) -> Self {
        Self::from(&value)
    }
}

impl<BlockCommitter> BlockCommitterDaBlockCosts<BlockCommitter> {
    /// Create a new instance of the block committer da block costs source
    pub fn new(client: BlockCommitter) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl<BlockCommitter> DaBlockCostsSource for BlockCommitterDaBlockCosts<BlockCommitter>
where
    BlockCommitter: BlockCommitterApi,
{
    async fn request_da_block_costs(
        &mut self,
        last_recorded_height: &BlockHeight,
    ) -> DaBlockCostsResult<Vec<DaBlockCosts>> {
        let next_height = last_recorded_height.succ().ok_or(anyhow!(
            "Failed to increment the last recorded height: {:?}",
            last_recorded_height
        ))?;

        let raw_da_block_costs: Vec<_> = self
            .client
            .get_costs_by_l2_block_number(*next_height.deref())
            .await?;

        let da_block_costs: Vec<_> =
            raw_da_block_costs.iter().map(DaBlockCosts::from).collect();

        Ok(da_block_costs)
    }
}

pub struct BlockCommitterHttpApi {
    client: reqwest::Client,
    url: Option<url::Url>,
}

impl BlockCommitterHttpApi {
    pub fn new(url: Option<url::Url>) -> Self {
        Self {
            client: reqwest::Client::new(),
            url,
        }
    }
}

const NUMBER_OF_BUNDLES: u32 = 10;
#[async_trait::async_trait]
impl BlockCommitterApi for BlockCommitterHttpApi {
    async fn get_costs_by_l2_block_number(
        &self,
        l2_block_number: u32,
    ) -> DaBlockCostsResult<Vec<RawDaBlockCosts>> {
        // Specific: http://committer.url/v1/costs?variant=specific&value=19098935&limit=5
        if let Some(url) = &self.url {
            tracing::debug!("getting da costs by l2 block number: {l2_block_number}");
            let path = format!("/v1/costs?variant=specific&value={l2_block_number}&limit={NUMBER_OF_BUNDLES}");
            let full_path = url.join(&path)?;
            let response = self.client.get(full_path).send().await?;
            let text = response.text().await?;
            let parsed: Vec<RawDaBlockCosts> = serde_json::from_str(&text).map_err(|e| { anyhow::anyhow!("Failed to get costs from block committer: {e} for the response {text}") })?;
            Ok(parsed)
        } else {
            Ok(vec![])
        }
    }
}

#[cfg(test)]
mod test_block_committer_http_api {
    #![allow(non_snake_case)]

    use super::*;
    use fake_server::FakeServer;

    #[test]
    fn get_costs_by_l2_block_number__when_url_is_none__then_returns_empty_vec() {
        let rt = tokio::runtime::Runtime::new().unwrap();

        // given
        let block_committer = BlockCommitterHttpApi::new(None);
        let l2_block_number = 1;

        // when
        let actual = rt.block_on(async {
            block_committer
                .get_costs_by_l2_block_number(l2_block_number)
                .await
                .unwrap()
        });

        // then
        assert_eq!(actual.len(), 0);
    }
    #[test]
    fn get_costs_by_l2_block_number__when_url_is_some__then_returns_expected_costs() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut mock = FakeServer::new();
        let url = mock.url();

        // given
        let l2_block_number = 51;
        let block_committer = BlockCommitterHttpApi::new(Some(url));

        let too_early_count = 5;
        let do_not_fit_count = 5;

        let mut current_height = 0;
        let mut bundle_id = 0;
        let mut da_block_height: u64 = 0;

        // shouldn't return
        for _ in 0..too_early_count {
            bundle_id += 1;
            da_block_height += 1;
            current_height += 1;
            let start_height = current_height;
            current_height += 9;
            let end_height = current_height;
            let costs = RawDaBlockCosts {
                id: bundle_id,
                start_height,
                end_height,
                da_block_height: DaBlockHeight::from(da_block_height),
                cost: 1,
                size: 1,
            };
            mock.add_response(costs);
        }
        let mut expected = Vec::new();

        // should return
        for _ in 0..NUMBER_OF_BUNDLES {
            bundle_id += 1;
            da_block_height += 1;
            current_height += 1;
            let start_height = current_height;
            current_height += 9;
            let end_height = current_height;
            let costs = RawDaBlockCosts {
                id: bundle_id,
                start_height,
                end_height,
                da_block_height: DaBlockHeight::from(da_block_height),
                cost: 1,
                size: 1,
            };
            mock.add_response(costs.clone());
            expected.push(costs);
        }
        // don't fit
        for _ in 0..do_not_fit_count {
            bundle_id += 1;
            da_block_height += 1;
            current_height += 1;
            let start_height = current_height;
            current_height += 9;
            let end_height = current_height;
            let costs = RawDaBlockCosts {
                id: bundle_id,
                start_height,
                end_height,
                da_block_height: DaBlockHeight::from(da_block_height),
                cost: 1,
                size: 1,
            };
            mock.add_response(costs);
        }

        // when
        mock.init();
        let actual = rt.block_on(async {
            block_committer
                .get_costs_by_l2_block_number(l2_block_number)
                .await
                .unwrap()
        });

        // then
        assert_eq!(actual, expected);
    }
}
#[cfg(any(test, feature = "test-helpers"))]
pub mod fake_server {
    use crate::v1::da_source_service::block_committer_costs::RawDaBlockCosts;
    use mockito::Matcher::Any;
    use std::{
        collections::HashMap,
        sync::{
            Arc,
            Mutex,
        },
    };

    pub struct FakeServer {
        server: mockito::ServerGuard,
        responses: Arc<Mutex<Vec<RawDaBlockCosts>>>,
    }

    impl FakeServer {
        pub fn new() -> Self {
            let server = mockito::Server::new();
            let responses = Arc::new(Mutex::new(Vec::new()));
            let mut fake = Self { server, responses };
            fake.init();
            fake
        }

        #[allow(unused_variables)]
        pub fn init(&mut self) {
            let shared_responses = self.responses.clone();
            self.server
                .mock("GET", Any)
                .with_status(201)
                .with_body_from_request(move |request| {
                    // take the requested number and return the corresponding response from the `responses` hashmap
                    let path = request.path_and_query();
                    tracing::info!("Path: {:?}", path);
                    let query = path.split("variant=").last().unwrap();
                    tracing::info!("Query: {:?}", query);
                    let mut values = query.split('&');
                    let variant = values.next().unwrap();
                    tracing::info!("Variant: {:?}", variant);
                    match variant {
                        // Latest: http://localhost:8080/v1/costs?variant=latest&limit=5
                        // We don't support `limit` yet!!!!
                        "latest" => {
                            let args = values.next().unwrap();
                            let limit =
                                args.split('=').last().unwrap().parse::<usize>().unwrap();
                            assert!(limit == 1);
                            let guard = shared_responses.lock().unwrap();
                            let most_recent = guard
                                .iter()
                                .fold(None, |acc, x| match acc {
                                    None => Some(x),
                                    Some(acc) => {
                                        if x.end_height > acc.end_height {
                                            Some(x)
                                        } else {
                                            Some(acc)
                                        }
                                    }
                                })
                                .cloned();
                            let response: Vec<RawDaBlockCosts> =
                                most_recent.into_iter().collect();
                            serde_json::to_string(&response).unwrap().into()
                        }
                        // Specific: http://localhost:8080/v1/costs?variant=specific&value=19098935&limit=5
                        "specific" => {
                            let args = values.next().unwrap();
                            let mut specific_values = args.split('=');
                            let height =
                                specific_values.last().unwrap().parse::<u32>().unwrap();
                            tracing::info!("Height: {:?}", height);
                            let maybe_limit = values
                                .next()
                                .and_then(|x| x.split('=').last())
                                .and_then(|x| x.parse::<usize>().ok());
                            tracing::info!("Limit: {:?}", maybe_limit);
                            let guard = shared_responses.lock().unwrap();
                            let response = guard
                                .iter()
                                .filter(|costs| costs.end_height >= height)
                                .take(maybe_limit.unwrap_or(usize::MAX))
                                .cloned()
                                .collect::<Vec<_>>();
                            serde_json::to_string(&response).unwrap().into()
                        }
                        _ => {
                            panic!("Invalid variant: {}", variant);
                        }
                    }
                })
                .expect_at_least(1)
                .create();
        }

        pub fn add_response(&mut self, costs: RawDaBlockCosts) {
            let mut guard = self.responses.lock().unwrap();
            guard.push(costs);
        }

        pub fn url(&self) -> url::Url {
            url::Url::parse(self.server.url().as_str()).unwrap()
        }
    }
    impl Default for FakeServer {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    struct MockBlockCommitterApi {
        value: Option<RawDaBlockCosts>,
    }

    impl MockBlockCommitterApi {
        fn new(value: Option<RawDaBlockCosts>) -> Self {
            Self { value }
        }
    }

    #[async_trait::async_trait]
    impl BlockCommitterApi for MockBlockCommitterApi {
        async fn get_costs_by_l2_block_number(
            &self,
            l2_block_number: u32,
        ) -> DaBlockCostsResult<Vec<RawDaBlockCosts>> {
            // arbitrary logic to generate a new value
            let mut value = self.value.clone();
            if let Some(value) = &mut value {
                value.start_height = l2_block_number;
                value.end_height = value.end_height + l2_block_number + 10;
                value.da_block_height =
                    value.da_block_height + ((l2_block_number + 1) as u64).into();
                value.cost += 1;
                value.size += 1;
            }
            Ok(value.into_iter().collect())
        }
    }

    fn test_da_block_costs() -> RawDaBlockCosts {
        RawDaBlockCosts {
            id: 1,
            start_height: 1,
            end_height: 10,
            da_block_height: 1u64.into(),
            cost: 1,
            size: 1,
        }
    }

    #[tokio::test]
    async fn request_da_block_cost__when_last_value_is_some__then_get_costs_by_l2_block_number_is_called(
    ) {
        // given
        let mut da_block_costs = test_da_block_costs();
        let da_block_costs_len = da_block_costs.end_height - da_block_costs.start_height;
        let mock_api = MockBlockCommitterApi::new(Some(da_block_costs.clone()));
        let latest_height = BlockHeight::new(da_block_costs.end_height);
        let mut block_committer = BlockCommitterDaBlockCosts::new(mock_api);

        // when
        let actual = block_committer
            .request_da_block_costs(&latest_height)
            .await
            .unwrap();

        // then
        let l2_blocks = actual.first().unwrap().l2_blocks.clone();
        let range_len = l2_blocks.end() - l2_blocks.start();
        assert_ne!(da_block_costs_len, range_len);
    }
}
