use crate::{
    block_range_response::BlockRangeResponse,
    blocks::BlockSourceEvent,
    db::{BlockAggregatorDB, table::LatestBlock},
    protobuf_types::Block as ProtoBlock,
    result::Error,
};
use anyhow::anyhow;
use aws_config::{
    BehaviorVersion, default_provider::credentials::DefaultCredentialsChain,
};
use aws_sdk_s3::{self, Client, primitives::ByteStream};
use flate2::{Compression, write::GzEncoder};
use fuel_core_storage::{
    Error as StorageError, StorageAsMut, StorageAsRef, StorageInspect, StorageMutate,
    transactional::{Modifiable, StorageTransaction, WriteTransaction},
};
use fuel_core_types::fuel_types::BlockHeight;
use prost::Message;
use std::io::Write;

#[allow(non_snake_case)]
#[cfg(test)]
mod tests;

#[allow(unused)]
pub struct RemoteCache<S> {
    // aws configuration
    aws_bucket: String,
    requester_pays: bool,
    aws_endpoint: Option<String>,
    client: Option<Client>,

    // track consistency between runs
    local_persisted: S,
    sync_from: BlockHeight,
    highest_new_height: Option<BlockHeight>,
    orphaned_new_height: Option<BlockHeight>,
    synced: bool,
}

impl<S> RemoteCache<S> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        aws_bucket: String,
        requester_pays: bool,
        aws_endpoint: Option<String>,
        client: Option<Client>,
        local_persisted: S,
        sync_from: BlockHeight,
    ) -> RemoteCache<S> {
        RemoteCache {
            aws_bucket,
            requester_pays,
            aws_endpoint,
            client,
            local_persisted,
            sync_from,
            highest_new_height: None,
            orphaned_new_height: None,
            synced: false,
        }
    }

    async fn client(&mut self) -> crate::result::Result<&Client> {
        self.init_client().await;
        self.client
            .as_ref()
            .ok_or(Error::db_error(anyhow!("AWS S3 client is uninitialized")))
    }

    // only runs the first time
    async fn init_client(&mut self) {
        if self.client.is_none() {
            let credentials = DefaultCredentialsChain::builder().build().await;
            let sdk_config = aws_config::defaults(BehaviorVersion::latest())
                .credentials_provider(credentials)
                .load()
                .await;
            let mut config_builder = aws_sdk_s3::config::Builder::from(&sdk_config);
            if let Some(endpoint) = &self.aws_endpoint {
                config_builder.set_endpoint_url(Some(endpoint.to_string()));
            }
            let config = config_builder.force_path_style(true).build();
            let client = aws_sdk_s3::Client::from_conf(config);
            self.client = Some(client);
        }
    }
}

impl<S> BlockAggregatorDB for RemoteCache<S>
where
    S: Send + Sync,
    S: Modifiable,
    S: StorageInspect<LatestBlock, Error = StorageError>,
    for<'b> StorageTransaction<&'b mut S>:
        StorageMutate<LatestBlock, Error = StorageError>,
{
    type Block = ProtoBlock;
    type BlockRangeResponse = BlockRangeResponse;

    async fn store_block(
        &mut self,
        block_event: BlockSourceEvent<Self::Block>,
    ) -> crate::result::Result<()> {
        let (height, block) = block_event.clone().into_inner();
        let key = block_height_to_key(&height);
        let mut buf = Vec::new();
        block.encode(&mut buf).map_err(Error::db_error)?;
        let zipped = gzip_bytes(&buf)?;
        let body = ByteStream::from(zipped);
        let req = self
            .client()
            .await?
            .put_object()
            .bucket(&self.aws_bucket)
            .key(&key)
            .body(body)
            .content_encoding("gzip")
            .content_type("application/octet-stream");
        let _ = req.send().await.map_err(Error::db_error)?;
        match block_event {
            BlockSourceEvent::NewBlock(new_height, _) => {
                tracing::debug!("New block: {:?}", new_height);
                tracing::info!("New block: {:?}", new_height);
                self.highest_new_height = Some(new_height);
                if self.synced {
                    tracing::info!("Updating latest block to {:?}", new_height);
                    let mut tx = self.local_persisted.write_transaction();
                    tx.storage_as_mut::<LatestBlock>()
                        .insert(&(), &new_height)
                        .map_err(|e| Error::DB(anyhow!(e)))?;
                    tx.commit().map_err(|e| Error::DB(anyhow!(e)))?;
                } else if new_height == self.sync_from {
                    tracing::info!("Updating latest block to {:?}", new_height);
                    self.synced = true;
                    self.highest_new_height = Some(new_height);
                    self.orphaned_new_height = None;
                    let mut tx = self.local_persisted.write_transaction();
                    tx.storage_as_mut::<LatestBlock>()
                        .insert(&(), &new_height)
                        .map_err(|e| Error::DB(anyhow!(e)))?;
                    tx.commit().map_err(|e| Error::DB(anyhow!(e)))?;
                } else if self.height_is_next_height(new_height)? {
                    tracing::info!("Updating latest block to {:?}", new_height);
                    self.synced = true;
                    self.highest_new_height = Some(new_height);
                    self.orphaned_new_height = None;
                    let mut tx = self.local_persisted.write_transaction();
                    tx.storage_as_mut::<LatestBlock>()
                        .insert(&(), &new_height)
                        .map_err(|e| Error::DB(anyhow!(e)))?;
                    tx.commit().map_err(|e| Error::DB(anyhow!(e)))?;
                } else if self.orphaned_new_height.is_none() {
                    tracing::info!("Marking block as orphaned: {:?}", new_height);
                    self.orphaned_new_height = Some(new_height);
                }
            }
            BlockSourceEvent::OldBlock(height, _) => {
                tracing::debug!("Old block: {:?}", height);
                tracing::info!("Old block: {:?}", height);
                let mut tx = self.local_persisted.write_transaction();
                let latest_height = if height.succ() == self.orphaned_new_height {
                    tracing::info!("Marking block as synced: {:?}", height);
                    self.orphaned_new_height = None;
                    self.synced = true;
                    self.highest_new_height.unwrap_or(height)
                } else {
                    tracing::info!("Updating latest block to {:?}", height);
                    height
                };
                tx.storage_as_mut::<LatestBlock>()
                    .insert(&(), &latest_height)
                    .map_err(|e| Error::DB(anyhow!(e)))?;
                tx.commit().map_err(|e| Error::DB(anyhow!(e)))?;
            }
        }
        Ok(())
    }

    async fn get_block_range(
        &self,
        first: BlockHeight,
        last: BlockHeight,
    ) -> crate::result::Result<Self::BlockRangeResponse> {
        // TODO: Check if it exists
        let bucket = self.aws_bucket.clone();
        let requester_pays = self.requester_pays;
        let aws_endpoint = self.aws_endpoint.clone();

        let stream = futures::stream::iter((*first..=*last).map(move |height| {
            let block_height = BlockHeight::new(height);
            let key = block_height_to_key(&block_height);
            let res = crate::block_range_response::RemoteS3Response {
                bucket: bucket.clone(),
                key: key.clone(),
                requester_pays,
                aws_endpoint: aws_endpoint.clone(),
            };
            (block_height, res)
        }));
        Ok(BlockRangeResponse::S3(Box::pin(stream)))
    }

    async fn get_current_height(&self) -> crate::result::Result<Option<BlockHeight>> {
        tracing::debug!("Getting current height from local cache");
        let height = self
            .local_persisted
            .storage_as_ref::<LatestBlock>()
            .get(&())
            .map_err(|e| Error::DB(anyhow!(e)))?;

        Ok(height.map(|b| b.into_owned()))
    }
}

impl<S> RemoteCache<S>
where
    S: Send + Sync,
    S: StorageInspect<LatestBlock, Error = StorageError>,
    for<'b> StorageTransaction<&'b mut S>:
        StorageMutate<LatestBlock, Error = StorageError>,
{
    fn height_is_next_height(&self, height: BlockHeight) -> crate::result::Result<bool> {
        let maybe_latest_height = self
            .local_persisted
            .storage_as_ref::<LatestBlock>()
            .get(&())
            .map_err(|e| Error::DB(anyhow!(e)))?;
        if let Some(latest_height) = maybe_latest_height {
            Ok(latest_height.succ() == Some(height))
        } else {
            Ok(false)
        }
    }
}

pub fn block_height_to_key(height: &BlockHeight) -> String {
    let raw: [u8; 4] = height.to_bytes();
    format!(
        "{:02}/{:02}/{:02}/{:02}",
        &raw[0], &raw[1], &raw[2], &raw[3]
    )
}

pub fn gzip_bytes(data: &[u8]) -> crate::result::Result<Vec<u8>> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).map_err(Error::db_error)?;
    encoder.finish().map_err(Error::db_error)
}
