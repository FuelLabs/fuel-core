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
    /// Used on first run to get the latest costs and seqno
    async fn get_latest_costs(&self) -> DaBlockCostsResult<Option<RawDaBlockCosts>>;
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
    last_recorded_height: Option<BlockHeight>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq)]
pub struct RawDaBlockCosts {
    pub bundle_id: u32,
    /// The beginning of the range of blocks that the costs apply to
    pub start_height: u32,
    /// The end of the range of blocks that the costs apply to
    pub end_height: u32,
    /// The DA block height of the last transaction for the range of blocks
    pub da_block_height: DaBlockHeight,
    /// cost of posting this blob (wei)
    pub cost_wei: u128,
    /// size of this blob (bytes)
    pub size_bytes: u32,
}

impl From<&RawDaBlockCosts> for DaBlockCosts {
    fn from(raw_da_block_costs: &RawDaBlockCosts) -> Self {
        let RawDaBlockCosts {
            start_height,
            end_height,
            cost_wei,
            size_bytes,
            bundle_id,
            ..
        } = *raw_da_block_costs;
        DaBlockCosts {
            bundle_id,
            // construct a vec of l2 blocks from the start_height to the end_height
            l2_blocks: (start_height..=end_height).collect(),
            bundle_size_bytes: size_bytes,
            blob_cost_wei: cost_wei,
        }
    }
}

impl<BlockCommitter> BlockCommitterDaBlockCosts<BlockCommitter> {
    /// Create a new instance of the block committer da block costs source
    pub fn new(
        client: BlockCommitter,
        last_recorded_height: Option<BlockHeight>,
    ) -> Self {
        Self {
            client,
            last_recorded_height,
        }
    }
}

#[async_trait::async_trait]
impl<BlockCommitter> DaBlockCostsSource for BlockCommitterDaBlockCosts<BlockCommitter>
where
    BlockCommitter: BlockCommitterApi,
{
    async fn request_da_block_cost(&mut self) -> DaBlockCostsResult<Vec<DaBlockCosts>> {
        let raw_da_block_costs: Vec<_> =
            match self.last_recorded_height.and_then(|x| x.succ()) {
                Some(ref next_height) => {
                    self.client
                        .get_costs_by_l2_block_number(*next_height.deref())
                        .await?
                }
                _ => self.client.get_latest_costs().await?.into_iter().collect(),
            };

        tracing::info!("raw_da_block_costs: {:?}", raw_da_block_costs);
        let da_block_costs: Vec<_> = raw_da_block_costs
            .iter()
            .map(|raw| DaBlockCosts::from(raw))
            .collect();
        tracing::info!("da_block_costs: {:?}", da_block_costs);
        if let Some(cost) = raw_da_block_costs.last() {
            self.last_recorded_height = Some(BlockHeight::from(cost.end_height));
        }

        Ok(da_block_costs)
    }

    async fn set_last_value(&mut self, height: BlockHeight) -> DaBlockCostsResult<()> {
        self.last_recorded_height = Some(height);
        Ok(())
    }
}

impl From<RawDaBlockCosts> for DaBlockCosts {
    fn from(value: RawDaBlockCosts) -> Self {
        Self {
            bundle_id: value.bundle_id,
            l2_blocks: (value.start_height..=value.end_height).collect(),
            bundle_size_bytes: value.size_bytes,
            blob_cost_wei: value.cost_wei,
        }
    }
}

pub struct BlockCommitterHttpApi {
    client: reqwest::Client,
    url: Option<String>,
}

impl BlockCommitterHttpApi {
    pub fn new(url: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            url,
        }
    }
}

const PAGE_SIZE: u32 = 10;
#[async_trait::async_trait]
impl BlockCommitterApi for BlockCommitterHttpApi {
    async fn get_costs_by_l2_block_number(
        &self,
        l2_block_number: u32,
    ) -> DaBlockCostsResult<Vec<RawDaBlockCosts>> {
        // Specific: http://localhost:8080/v1/costs?variant=specific&value=19098935&limit=5
        if let Some(url) = &self.url {
            let formatted_url = format!("{url}/v1/costs?variant=specific&value={l2_block_number}&limit={PAGE_SIZE}");
            let val = self.client.get(formatted_url).send().await?;
            tracing::warn!("val: {:?}", val);
            let response = val.json::<Vec<RawDaBlockCosts>>().await?;
            tracing::warn!("Response: {:?}", response);
            Ok(response)
        } else {
            Ok(vec![])
        }
    }

    async fn get_latest_costs(&self) -> DaBlockCostsResult<Option<RawDaBlockCosts>> {
        // Latest: http://localhost:8080/v1/costs?variant=latest&limit=5
        if let Some(url) = &self.url {
            let formatted_url = format!("{url}/v1/costs?variant=latest&limit=1");
            let val = self.client.get(formatted_url).send().await?;
            tracing::warn!("val: {:?}", val);
            let response = val.json::<Vec<RawDaBlockCosts>>().await?;
            tracing::warn!("Response: {:?}", response);
            Ok(response.first().cloned())
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod test_block_committer_http_api {
    #![allow(non_snake_case)]

    use super::*;
    use crate::v1::da_source_service::block_committer_costs::fake_server::FakeServer;

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
                bundle_id,
                start_height,
                end_height,
                da_block_height: DaBlockHeight::from(da_block_height),
                cost_wei: 1,
                size_bytes: 1,
            };
            mock.add_response(costs);
        }
        let mut expected = Vec::new();

        // should return
        for _ in 0..PAGE_SIZE {
            bundle_id += 1;
            da_block_height += 1;
            current_height += 1;
            let start_height = current_height;
            current_height += 9;
            let end_height = current_height;
            let costs = RawDaBlockCosts {
                bundle_id,
                start_height,
                end_height,
                da_block_height: DaBlockHeight::from(da_block_height),
                cost_wei: 1,
                size_bytes: 1,
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
                bundle_id,
                start_height,
                end_height,
                da_block_height: DaBlockHeight::from(da_block_height),
                cost_wei: 1,
                size_bytes: 1,
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

    #[test]
    fn get_latest_costs__when_url_is_none__then_returns_none() {
        let rt = tokio::runtime::Runtime::new().unwrap();

        // given
        let block_committer = BlockCommitterHttpApi::new(None);

        // when
        let actual =
            rt.block_on(async { block_committer.get_latest_costs().await.unwrap() });

        // then
        assert_eq!(actual, None);
    }

    #[test]
    fn get_latest_costs__when_url_is_some__then_returns_expected_costs() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut mock = FakeServer::new();
        let url = mock.url();

        // given
        let block_committer = BlockCommitterHttpApi::new(Some(url));
        let not_expected = RawDaBlockCosts {
            bundle_id: 1,
            start_height: 1,
            end_height: 10,
            da_block_height: 1u64.into(),
            cost_wei: 1,
            size_bytes: 1,
        };
        mock.add_response(not_expected);
        let expected = RawDaBlockCosts {
            bundle_id: 2,
            start_height: 11,
            end_height: 20,
            da_block_height: 2u64.into(),
            cost_wei: 2,
            size_bytes: 2,
        };
        mock.add_response(expected.clone());

        // when
        let actual =
            rt.block_on(async { block_committer.get_latest_costs().await.unwrap() });

        // then
        assert_eq!(actual, Some(expected));
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
                                .map(|c| c.clone())
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

        pub fn url(&self) -> String {
            self.server.url()
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
        async fn get_latest_costs(&self) -> DaBlockCostsResult<Option<RawDaBlockCosts>> {
            Ok(self.value.clone())
        }
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
                value.cost_wei += 1;
                value.size_bytes += 1;
            }
            Ok(value.into_iter().collect())
        }
    }

    fn test_da_block_costs() -> RawDaBlockCosts {
        RawDaBlockCosts {
            bundle_id: 1,
            start_height: 1,
            end_height: 10,
            da_block_height: 1u64.into(),
            cost_wei: 1,
            size_bytes: 1,
        }
    }

    #[tokio::test]
    async fn request_da_block_cost__when_last_value_is_none__then_get_latest_costs_is_called(
    ) {
        // given
        let da_block_costs = test_da_block_costs();
        let expected = vec![(&da_block_costs).into()];
        let mock_api = MockBlockCommitterApi::new(Some(da_block_costs));
        let mut block_committer = BlockCommitterDaBlockCosts::new(mock_api, None);

        // when
        let actual = block_committer.request_da_block_cost().await.unwrap();

        // then
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn request_da_block_cost__when_last_value_is_some__then_get_costs_by_seqno_is_called(
    ) {
        // given
        let mut da_block_costs = test_da_block_costs();
        let da_block_costs_len = da_block_costs.end_height - da_block_costs.start_height;
        let mock_api = MockBlockCommitterApi::new(Some(da_block_costs.clone()));
        let latest_height = BlockHeight::new(da_block_costs.end_height);
        let mut block_committer =
            BlockCommitterDaBlockCosts::new(mock_api, Some(latest_height));

        // when
        let actual = block_committer.request_da_block_cost().await.unwrap();

        // then
        assert_ne!(
            da_block_costs_len as usize,
            actual.first().unwrap().l2_blocks.len()
        );
    }
}
