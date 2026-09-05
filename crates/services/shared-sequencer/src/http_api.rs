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
    #[derive(Debug, Deserialize)]
    pub struct RpcResponse<T> {
        pub jsonrpc: String,
        pub id: u64,
        pub result: Option<T>,
        pub error: Option<serde_json::Value>,
    }

    #[derive(Debug, Deserialize)]
    pub struct AbciInfoResponse {
        pub response: AbciInfo,
    }

    #[derive(Debug, Deserialize)]
    pub struct AbciInfo {
        pub last_block_height: String,
    }

    #[derive(Debug, Deserialize)]
    pub struct BroadcastTxResponse {
        pub code: u32,
        pub log: String,
    }
}

#[derive(Debug, serde::Serialize)]
struct RpcRequest<P> {
    jsonrpc: &'static str,
    id: u64,
    method: &'static str,
    params: P,
}

#[derive(Debug, serde::Serialize)]
struct EmptyParams {}

#[derive(Debug, serde::Serialize)]
struct BroadcastTxParams {
    tx: String,
}

async fn rpc<T, P>(
    http: &reqwest::Client,
    api_url: &str,
    method: &'static str,
    params: P,
) -> anyhow::Result<T>
where
    T: serde::de::DeserializeOwned,
    P: serde::Serialize,
{
    let request = RpcRequest {
        jsonrpc: "2.0",
        id: 1,
        method,
        params,
    };
    let response = http.post(api_url).json(&request).send().await?;
    if response.status() != reqwest::StatusCode::OK {
        anyhow::bail!(
            "Tendermint RPC request failed with HTTP status {}",
            response.status()
        );
    }
    let text = response.text().await?;
    let response: api_types::RpcResponse<T> =
        serde_json::from_str(&text).with_context(|| format!("response text {text}"))?;

    if response.jsonrpc != request.jsonrpc {
        anyhow::bail!(
            "Tendermint RPC response uses unsupported JSON-RPC version {}",
            response.jsonrpc
        );
    }
    if response.id != request.id {
        anyhow::bail!(
            "Tendermint RPC response ID {} does not match request ID {}",
            response.id,
            request.id
        );
    }

    if let Some(error) = response.error {
        anyhow::bail!("Tendermint RPC error: {error}");
    }

    response
        .result
        .ok_or_else(|| anyhow::anyhow!("Tendermint RPC response has no result"))
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
    http: &reqwest::Client,
    api_url: &str,
    tx_bytes: Vec<u8>,
) -> anyhow::Result<u64> {
    let tx_bytes = BASE64_STANDARD.encode(&tx_bytes);
    let request = SimulateRequest {
        tx_bytes: tx_bytes.to_string(),
    };
    let path = "/cosmos/tx/v1beta1/simulate";
    let full_url = Url::parse(api_url)?.join(path).unwrap();
    let r = http.post(full_url).json(&request).send().await?;
    let text = r.text().await?;
    let resp: api_types::SimulateResponse =
        serde_json::from_str(&text).with_context(|| format!("response text {text}"))?;
    Ok(resp.gas_info.gas_used.parse()?)
}

pub async fn get_account_prefix(
    http: &reqwest::Client,
    api_url: &str,
) -> anyhow::Result<String> {
    let path = "/cosmos/auth/v1beta1/bech32";
    let full_url = Url::parse(api_url)?.join(path).unwrap();
    let r = http.get(full_url).send().await?;
    let text = r.text().await?;
    let resp: api_types::AccountPrefix =
        serde_json::from_str(&text).with_context(|| format!("response text {text}"))?;
    Ok(resp.bech32_prefix)
}

pub async fn chain_id(http: &reqwest::Client, api_url: &str) -> anyhow::Result<String> {
    let path = "/cosmos/base/tendermint/v1beta1/node_info";
    let full_url = Url::parse(api_url)?.join(path).unwrap();
    let r = http.get(full_url).send().await?;
    let text = r.text().await?;
    let resp: api_types::NodeInfo =
        serde_json::from_str(&text).with_context(|| format!("response text {text}"))?;
    Ok(resp.default_node_info.network)
}

pub async fn config(
    http: &reqwest::Client,
    api_url: &str,
) -> anyhow::Result<api_types::Config> {
    let path = "/cosmos/base/node/v1beta1/config";
    let full_url = Url::parse(api_url)?.join(path).unwrap();
    let r = http.get(full_url).send().await?;
    let text = r.text().await?;
    let resp: api_types::Config =
        serde_json::from_str(&text).with_context(|| format!("response text {text}"))?;
    Ok(resp)
}

pub async fn coin_denom(http: &reqwest::Client, api_url: &str) -> anyhow::Result<String> {
    let path = "/cosmos/staking/v1beta1/params";
    let full_url = Url::parse(api_url)?.join(path).unwrap();
    let r = http.get(full_url).send().await?;
    let text = r.text().await?;
    let resp: api_types::StakingParams =
        serde_json::from_str(&text).with_context(|| format!("response text {text}"))?;
    Ok(resp.params.bond_denom)
}

pub async fn latest_block_height(
    http: &reqwest::Client,
    api_url: &str,
) -> anyhow::Result<u32> {
    let response: api_types::AbciInfoResponse =
        rpc(http, api_url, "abci_info", EmptyParams {}).await?;
    Ok(response.response.last_block_height.parse()?)
}

pub async fn broadcast_tx_sync(
    http: &reqwest::Client,
    api_url: &str,
    tx_bytes: Vec<u8>,
) -> anyhow::Result<api_types::BroadcastTxResponse> {
    let tx = BASE64_STANDARD.encode(tx_bytes);
    rpc(http, api_url, "broadcast_tx_sync", BroadcastTxParams { tx }).await
}

pub async fn get_account(
    http: &reqwest::Client,
    api_url: &str,
    id: AccountId,
) -> anyhow::Result<AccountMetadata> {
    let path = format!("/cosmos/auth/v1beta1/accounts/{id}");
    let full_url = Url::parse(api_url)?.join(&path).unwrap();
    let r = http.get(full_url).send().await?;
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

pub async fn get_topic(
    http: &reqwest::Client,
    api_url: &str,
    id: [u8; 32],
) -> anyhow::Result<Option<TopicInfo>> {
    let id_b64 = BASE64_STANDARD.encode(id);
    let path = format!("/fuelsequencer/sequencing/v1/topic/{id_b64}");
    let full_url = Url::parse(api_url)?.join(&path).unwrap();
    let r = http.get(full_url).send().await?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Matcher;
    use serde_json::json;

    #[tokio::test]
    async fn latest_block_height_uses_tendermint_json_rpc_shape() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/")
            .match_body(Matcher::Json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "abci_info",
                "params": {},
            })))
            .with_status(200)
            .with_body(
                r#"{"jsonrpc":"2.0","id":1,"result":{"response":{"last_block_height":"154"}}}"#,
            )
            .create_async()
            .await;

        let height = latest_block_height(&reqwest::Client::new(), &server.url())
            .await
            .unwrap();

        assert_eq!(height, 154);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn broadcast_tx_sync_base64_encodes_transaction_bytes() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/")
            .match_body(Matcher::Json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "broadcast_tx_sync",
                "params": {"tx": "AQID"},
            })))
            .with_status(200)
            .with_body(r#"{"jsonrpc":"2.0","id":1,"result":{"code":0,"log":""}}"#)
            .create_async()
            .await;

        let response =
            broadcast_tx_sync(&reqwest::Client::new(), &server.url(), vec![1, 2, 3])
                .await
                .unwrap();

        assert_eq!(response.code, 0);
        assert!(response.log.is_empty());
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn latest_block_height_rejects_mismatched_response_id() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/")
            .with_status(200)
            .with_body(
                r#"{"jsonrpc":"2.0","id":2,"result":{"response":{"last_block_height":"154"}}}"#,
            )
            .create_async()
            .await;

        let err = latest_block_height(&reqwest::Client::new(), &server.url())
            .await
            .expect_err("mismatched response ID should fail");

        assert!(err.to_string().contains("does not match request ID"));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn latest_block_height_rejects_unsupported_json_rpc_version() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/")
            .with_status(200)
            .with_body(
                r#"{"jsonrpc":"1.0","id":1,"result":{"response":{"last_block_height":"154"}}}"#,
            )
            .create_async()
            .await;

        let err = latest_block_height(&reqwest::Client::new(), &server.url())
            .await
            .expect_err("unsupported JSON-RPC version should fail");

        assert!(err.to_string().contains("unsupported JSON-RPC version"));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn broadcast_tx_sync_requires_http_ok() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/")
            .with_status(201)
            .with_body(r#"{"jsonrpc":"2.0","id":1,"result":{"code":0,"log":""}}"#)
            .create_async()
            .await;

        let err =
            broadcast_tx_sync(&reqwest::Client::new(), &server.url(), vec![1, 2, 3])
                .await
                .expect_err("non-200 JSON-RPC response should fail");

        assert!(err.to_string().contains("HTTP status 201"));
        mock.assert_async().await;
    }
}
