use anyhow::Context;
use base64::prelude::*;
use cosmrs::AccountId;
use url::Url;

mod api_types {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    pub struct AccountResponse {
        pub account: AccountInfo,
    }

    #[derive(Debug, Deserialize)]
    pub struct AccountInfo {
        pub account_number: String,
        pub sequence: String,
    }

    #[derive(Debug, Deserialize)]
    pub struct AccountPrefix {
        pub bech32_prefix: String,
    }

    #[derive(Debug, Deserialize)]
    pub struct NodeInfo {
        pub default_node_info: DefaultNodeInfo,
    }

    #[derive(Debug, Deserialize)]
    pub struct DefaultNodeInfo {
        pub network: String,
    }

    #[derive(Debug, Deserialize)]
    pub struct StakingParams {
        pub params: StakingParamsInner,
    }

    #[derive(Debug, Deserialize)]
    pub struct StakingParamsInner {
        pub bond_denom: String,
    }

    #[derive(Debug, Deserialize)]
    pub struct TopicResponse {
        pub topic: TopicInfo,
    }

    #[derive(Debug, Deserialize)]
    pub struct TopicInfo {
        pub owner: String,
        pub order: String,
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct SimulateResponse {
        pub gas_info: GasInfo,
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct GasInfo {
        pub gas_used: String,
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct Config {
        pub minimum_gas_price: String,
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub struct AccountMetadata {
    pub account_number: u64,
    pub sequence: u64,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct SimulateRequest {
    pub tx_bytes: String,
}

pub async fn estimate_transaction(
    api_url: &str,
    tx_bytes: Vec<u8>,
) -> anyhow::Result<u64> {
    let tx_bytes = BASE64_STANDARD.encode(&tx_bytes);
    let request = SimulateRequest {
        tx_bytes: tx_bytes.to_string(),
    };
    let path = "/cosmos/tx/v1beta1/simulate";
    let full_url = Url::parse(api_url)?.join(path).unwrap();
    let r = reqwest::Client::new()
        .post(full_url)
        .json(&request)
        .send()
        .await?;
    let text = r.text().await?;
    let resp: api_types::SimulateResponse =
        serde_json::from_str(&text).with_context(|| format!("response text {text}"))?;
    Ok(resp.gas_info.gas_used.parse()?)
}

pub async fn get_account_prefix(api_url: &str) -> anyhow::Result<String> {
    let path = "/cosmos/auth/v1beta1/bech32";
    let full_url = Url::parse(api_url)?.join(path).unwrap();
    let r = reqwest::get(full_url).await?;
    let text = r.text().await?;
    let resp: api_types::AccountPrefix =
        serde_json::from_str(&text).with_context(|| format!("response text {text}"))?;
    Ok(resp.bech32_prefix)
}

pub async fn chain_id(api_url: &str) -> anyhow::Result<String> {
    let path = "/cosmos/base/tendermint/v1beta1/node_info";
    let full_url = Url::parse(api_url)?.join(path).unwrap();
    let r = reqwest::get(full_url).await?;
    let text = r.text().await?;
    let resp: api_types::NodeInfo =
        serde_json::from_str(&text).with_context(|| format!("response text {text}"))?;
    Ok(resp.default_node_info.network)
}

pub async fn config(api_url: &str) -> anyhow::Result<api_types::Config> {
    let path = "/cosmos/base/node/v1beta1/config";
    let full_url = Url::parse(api_url)?.join(path).unwrap();
    let r = reqwest::get(full_url).await?;
    let text = r.text().await?;
    let resp: api_types::Config =
        serde_json::from_str(&text).with_context(|| format!("response text {text}"))?;
    Ok(resp)
}

pub async fn coin_denom(api_url: &str) -> anyhow::Result<String> {
    let path = "/cosmos/staking/v1beta1/params";
    let full_url = Url::parse(api_url)?.join(path).unwrap();
    let r = reqwest::get(full_url).await?;
    let text = r.text().await?;
    let resp: api_types::StakingParams =
        serde_json::from_str(&text).with_context(|| format!("response text {text}"))?;
    Ok(resp.params.bond_denom)
}

pub async fn get_account(
    api_url: &str,
    id: AccountId,
) -> anyhow::Result<AccountMetadata> {
    let path = format!("/cosmos/auth/v1beta1/accounts/{id}");
    let full_url = Url::parse(api_url)?.join(&path).unwrap();
    let r = reqwest::get(full_url).await?;
    let text = r.text().await?;
    let resp: api_types::AccountResponse =
        serde_json::from_str(&text).with_context(|| format!("response text {text}"))?;
    let account_number = resp
        .account
        .account_number
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid account_number"))?;
    let sequence = resp
        .account
        .sequence
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid sequence"))?;
    Ok(AccountMetadata {
        account_number,
        sequence,
    })
}

#[derive(Debug)]
pub struct TopicInfo {
    pub owner: AccountId,
    pub order: u64,
}

pub async fn get_topic(api_url: &str, id: [u8; 32]) -> anyhow::Result<Option<TopicInfo>> {
    let id_b64 = BASE64_STANDARD.encode(id);
    let path = format!("/fuelsequencer/sequencing/v1/topic/{id_b64}");
    let full_url = Url::parse(api_url)?.join(&path).unwrap();
    let r = reqwest::get(full_url).await?;
    if r.status() == 404 {
        return Ok(None);
    }
    let text = r.text().await?;
    let resp: api_types::TopicResponse =
        serde_json::from_str(&text).with_context(|| format!("response text {text}"))?;
    let owner = resp
        .topic
        .owner
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid owner"))?;
    let order = resp
        .topic
        .order
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid order"))?;
    Ok(Some(TopicInfo { owner, order }))
}
