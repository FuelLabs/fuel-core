use std::ops::Range;

use fuel_core_types::fuel_types::BlockHeight;
use reqwest::{
    header::{
        HeaderMap,
        CONTENT_TYPE,
    },
    Url,
};

use crate::types::BlockCommitterCosts;

pub struct BlockCommitterDataFetcher {
    client: reqwest::Client,
    endpoint: Url,
    num_responses: usize,
}

impl BlockCommitterDataFetcher {
    pub fn new(endpoint: Url, num_responses: usize) -> Result<Self, anyhow::Error> {
        let mut content_type_json_header = HeaderMap::new();
        content_type_json_header.insert(
            CONTENT_TYPE,
            "application/json"
                .parse()
                .expect("Content-Type header value is valid"),
        );
        let client = reqwest::ClientBuilder::new()
            .default_headers(content_type_json_header)
            .build()?;
        Ok(Self {
            client,
            endpoint,
            num_responses,
        })
    }

    // Todo: Better error type
    async fn fetch_blob_data(
        &self,
        from_height: u64,
    ) -> Result<Vec<BlockCommitterCosts>, anyhow::Error> {
        let query = self.endpoint.join("v1/costs")?.join(&format!(
            "?variant=specific&value={}&limit={}",
            from_height, self.num_responses
        ))?;

        println!("Query: {}", query.as_str());

        let response = self.client.get(query).send().await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to fetch data from block committer: {}",
                response.status(),
            )
            .into());
        }

        let block_committer_costs = response.json::<Vec<BlockCommitterCosts>>().await?;
        Ok(block_committer_costs)
    }

    pub async fn fetch_l1_block_costs(
        &self,
        blocks: Range<BlockHeight>,
    ) -> Result<Vec<BlockCommitterCosts>, anyhow::Error> {
        let mut block_costs = vec![];
        let mut current_block_height = blocks.start;
        while current_block_height < blocks.end {
            let Ok(mut costs) =
                self.fetch_blob_data((*current_block_height).into()).await
            else {
                Err(anyhow::anyhow!(
                    "Could not fetch data for block {}",
                    current_block_height
                ))?
            };

            if costs.is_empty() {
                // Might be that the block committer doesn't have data for the block, in which case we return prematurely.
                // If this happen, we should increase the value of results returned by the block committer in the query.
                break;
            }

            // Block committer will return the data for the block in the next batch, hence we don't increment the height of the last
            // block.
            current_block_height = (*costs.last().unwrap().end_height).into();
            block_costs.append(&mut costs);
        }

        Ok(block_costs)
    }
}
