use crate::{
    blocks::BlockSource,
    result::{
        Error,
        Result,
    },
};
use fuel_core_storage::{
    Error as StorageError,
    StorageInspect,
    tables::{
        FuelBlocks,
        Transactions,
    },
};
use fuel_core_types::{
    blockchain::block::Block as FuelBlock,
    fuel_tx::{
        Receipt as FuelReceipt,
        Receipt,
        Transaction,
        TxId,
    },
    fuel_types::BlockHeight,
};
use std::sync::Arc;

pub mod convertor_adapter;

pub trait BlockConverter: Send + Sync + 'static {
    type Block;

    fn convert_block(
        &self,
        block: &FuelBlock,
        receipts: &[Vec<FuelReceipt>],
    ) -> Result<Self::Block>;
}

pub trait TxReceipts: Send + Sync + 'static {
    fn get_receipts(&self, tx_id: &TxId) -> Result<Vec<Receipt>>;
}

pub struct OldBlocksSource<Convertor, DB, Receipts> {
    convertor: Arc<Convertor>,
    db: Arc<DB>,
    receipts: Arc<Receipts>,
}

impl<Convertor, DB, Receipts> OldBlocksSource<Convertor, DB, Receipts> {
    pub fn new(convertor: Arc<Convertor>, db: DB, receipts: Receipts) -> Self {
        Self {
            convertor,
            db: Arc::new(db),
            receipts: Arc::new(receipts),
        }
    }
}

impl<Convertor, DB, Receipts> OldBlocksSource<Convertor, DB, Receipts>
where
    DB: Send + Sync + 'static,
    DB: StorageInspect<FuelBlocks, Error = StorageError>,
    DB: StorageInspect<Transactions, Error = StorageError>,
    Receipts: TxReceipts,
    Convertor: BlockConverter,
{
    pub fn blocks_stream_starting(
        &self,
        block_height: BlockHeight,
    ) -> impl Iterator<Item = Result<(BlockHeight, Convertor::Block)>> + Send + Sync + 'static
    {
        StorageIterator {
            convertor: self.convertor.clone(),
            db: self.db.clone(),
            receipts: self.receipts.clone(),
            next_height: Some(block_height),
        }
    }
}

impl<Convertor, DB, Receipts> BlockSource for OldBlocksSource<Convertor, DB, Receipts>
where
    DB: Send + Sync + 'static,
    DB: StorageInspect<FuelBlocks, Error = StorageError>,
    DB: StorageInspect<Transactions, Error = StorageError>,
    Receipts: TxReceipts,
    Convertor: BlockConverter,
{
    type Block = Convertor::Block;

    fn blocks_starting_from(
        &self,
        block_height: BlockHeight,
    ) -> impl Iterator<Item = Result<(BlockHeight, Self::Block)>> + Send + Sync + 'static
    {
        self.blocks_stream_starting(block_height)
    }
}

pub struct StorageIterator<Convertor, DB, Receipts> {
    convertor: Arc<Convertor>,
    db: Arc<DB>,
    receipts: Arc<Receipts>,
    next_height: Option<BlockHeight>,
}

impl<Convertor, DB, Receipts> StorageIterator<Convertor, DB, Receipts>
where
    DB: StorageInspect<FuelBlocks, Error = StorageError>,
    DB: StorageInspect<Transactions, Error = StorageError>,
    Receipts: TxReceipts,
    Convertor: BlockConverter,
{
    fn get_block_and_receipts(
        &self,
        height: &BlockHeight,
    ) -> Result<Option<(FuelBlock, Vec<Vec<Receipt>>)>> {
        let maybe_block = StorageInspect::<FuelBlocks>::get(self.db.as_ref(), height)
            .map_err(Error::block_source_error)?;
        if let Some(block) = maybe_block {
            let tx_ids = block.transactions();
            let txs = self.get_txs(tx_ids)?;
            let receipts = self.get_receipts(tx_ids)?;
            let block = block.into_owned().uncompress(txs);
            Ok(Some((block, receipts)))
        } else {
            Ok(None)
        }
    }

    fn get_txs(&self, tx_ids: &[TxId]) -> Result<Vec<Transaction>> {
        let mut txs = Vec::new();
        for tx_id in tx_ids {
            match StorageInspect::<Transactions>::get(self.db.as_ref(), tx_id)
                .map_err(Error::block_source_error)?
            {
                Some(tx) => {
                    tracing::debug!("found tx id: {:?}", tx_id);
                    txs.push(tx.into_owned());
                }
                None => {
                    return Ok(vec![]);
                }
            }
        }
        Ok(txs)
    }

    fn get_receipts(&self, tx_ids: &[TxId]) -> Result<Vec<Vec<Receipt>>> {
        use itertools::Itertools;
        tx_ids
            .iter()
            .map(|tx_id| {
                self.receipts.get_receipts(tx_id).map_err(|err| {
                    Error::DB(anyhow::anyhow!(err).context(format!(
                        "while getting receipts for tx_id: {:?}",
                        tx_id
                    )))
                })
            })
            .try_collect()
    }
}

impl<Convertor, DB, Receipts> Iterator for StorageIterator<Convertor, DB, Receipts>
where
    DB: StorageInspect<FuelBlocks, Error = StorageError>,
    DB: StorageInspect<Transactions, Error = StorageError>,
    Receipts: TxReceipts,
    Convertor: BlockConverter,
{
    type Item = Result<(BlockHeight, Convertor::Block)>;

    fn next(&mut self) -> Option<Self::Item> {
        let next_height = self.next_height?;

        let res = self.get_block_and_receipts(&next_height);
        match res {
            Ok(Some((block, receipts))) => {
                let block = match self.convertor.convert_block(&block, &receipts) {
                    Ok(b) => b,
                    Err(e) => return Some(Err(e)),
                };

                self.next_height = next_height.succ();
                Some(Ok((next_height, block)))
            }
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}
