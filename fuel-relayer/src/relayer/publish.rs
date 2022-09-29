use super::*;

pub async fn publish_fuel_block<P>(
    database: &mut dyn RelayerDb,
    eth_node: Arc<P>,
    contract: H160,
) -> anyhow::Result<()>
where
    P: Middleware<Error = ProviderError> + 'static,
{
    match get_blocks_for_publishing(database).await {
        Some((new_block, previous_block)) => {
            let eth_block = eth_node
                .get_block(*new_block.header.da_height)
                .await?
                .and_then(|eth_block| eth_block.hash);

            match eth_block {
                Some(eth_block) => {
                    database
                        .set_last_published_fuel_height(new_block.header.height)
                        .await;

                    publish_block(
                        contract,
                        eth_node.clone(),
                        new_block,
                        previous_block,
                        eth_block,
                    )
                    .await?;
                }
                None => {
                    tracing::warn!(
                        "Failed to get a eth block hash for fuel block for publishing"
                    )
                }
            }
        }
        None => tracing::warn!("Failed to get a fuel block for publishing"),
    }
    Ok(())
}

async fn get_blocks_for_publishing(
    database: &mut dyn RelayerDb,
) -> Option<(FuelBlock, FuelBlock)> {
    let height = database.get_chain_height().await;
    let new_block = database
        .get_sealed_block(height)
        .await
        .map(|b| b.block.clone());
    let previous_block = database
        .get_sealed_block(height.checked_sub(1)?.into())
        .await
        .map(|b| b.block.clone());
    new_block.zip(previous_block)
}

async fn publish_block<P>(
    contract: H160,
    eth_node: Arc<P>,
    new_block: FuelBlock,
    previous_block: FuelBlock,
    eth_block_hash: H256,
) -> anyhow::Result<()>
where
    P: Middleware<Error = ProviderError> + 'static,
{
    let client = crate::abi::fuel::Fuel::new(contract, eth_node);

    // minimum eth block height
    let eth_block_number: u32 = match (*new_block.header.da_height).try_into() {
        Ok(h) => h,
        Err(_) => {
            tracing::error!(
                "eth height was truncated to fit into u32 from: {}",
                new_block.header.da_height
            );
            *new_block.header.da_height as u32
        }
    };

    // Currently not using theses fields.
    let validators = Default::default();
    let stakes = Default::default();
    let signatures = Default::default();

    let txn = client.commit_block(
        eth_block_number,
        eth_block_hash.into(),
        new_block.into(),
        previous_block.into(),
        validators,
        stakes,
        signatures,
    );

    txn.send().await?;
    Ok(())
}

fn generate_message_root(
    block: FuelBlock,
) -> fuel_core_interfaces::common::prelude::Bytes32 {
    // The collect is needed to get an ExactSizeIterator
    #[allow(clippy::needless_collect)]
    let messages: Vec<_> = block
        .transactions
        .into_iter()
        .flat_map(|t| match t {
            fuel_core_interfaces::common::prelude::Transaction::Script {
                outputs,
                ..
            }
            | fuel_core_interfaces::common::prelude::Transaction::Create {
                outputs,
                ..
            } => outputs.into_iter().filter(Output::is_message),
        })
        .map(|mut message| {
            let mut buf = vec![0u8; message.serialized_size()];
            message.read_exact(&mut buf).expect(
                "This is safe because we have checked the size of buf is correct",
            );
            buf
        })
        .collect();

    crypto::ephemeral_merkle_root(messages.into_iter())
}

impl From<FuelBlock> for crate::abi::fuel::fuel::SidechainBlockHeader {
    fn from(block: FuelBlock) -> Self {
        Self {
            height: *block.header.height,
            previous_block_root: *block.header.prev_root,
            transaction_root: *block.header.transactions_root,
            message_outputs_root: *generate_message_root(block),
            // Currently not using these fields.
            validator_set_hash: Default::default(),
            required_stake: Default::default(),
        }
    }
}

pub(crate) async fn num_unpublished_messages(database: &dyn RelayerDb) -> usize {
    let mut height = database
        .get_last_published_fuel_height()
        .await
        .unwrap_or_default();

    let mut count = 0;

    while let Some(block) = database.get_sealed_block(height).await {
        count += block
            .block
            .transactions
            .iter()
            .map(|t| {
                t.outputs()
                    .iter()
                    .filter(|o| matches!(o, Output::Message { .. }))
                    .count()
            })
            .sum::<usize>();
        height = match height.checked_add(1) {
            Some(h) => h.into(),
            None => return count,
        }
    }
    count
}
