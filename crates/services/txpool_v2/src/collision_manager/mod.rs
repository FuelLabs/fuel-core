use fuel_core_types::fuel_tx::Output;

use crate::error::Error;

pub trait CollisionManager {
    fn add_outputs_transaction(&mut self, transaction: &Vec<Output>, ) -> Result<(), Error>;
}