use base64::prelude::*;
use cosmrs::AccountId;

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
    pub struct TopicResponse {
        pub topic: TopicInfo,
    }

    #[derive(Debug, Deserialize)]
    pub struct TopicInfo {
        pub owner: String,
        pub order: String,
    }
}

#[derive(Copy, Clone, Debug)]
pub struct AccountMetadata {
    pub account_number: u64,
    pub sequence: u64,
}

pub async fn get_account(
    api_url: &str,
    id: AccountId,
) -> anyhow::Result<AccountMetadata> {
    let r = reqwest::get(format!("{api_url}/cosmos/auth/v1beta1/accounts/{id}")).await?;
    let text = r.text().await?;
    let resp: api_types::AccountResponse = serde_json::from_str(&text)?;
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
    let r = reqwest::get(format!(
        "{api_url}/fuelsequencer/sequencing/v1/topic/{id_b64}"
    ))
    .await?;
    if r.status() == 404 {
        return Ok(None);
    }
    let text = r.text().await?;
    let resp: api_types::TopicResponse = serde_json::from_str(&text)?;
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
