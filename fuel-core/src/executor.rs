use fuel_core::database::SharedDatabase;
use fuel_core::model::block::Block;
use std::error::Error as StdError;
use thiserror::Error;

#[derive(Error)]
pub enum Error {
    BlockExecutionError(Box<dyn StdError>),
}

pub struct Executor {
    database: SharedDatabase,
}

impl Executor {
    pub async fn execute(&self, block: Block) -> Result<(), Error> {
        Ok(())
    }
}
