use std::sync::{
    Arc,
    RwLock,
};

pub struct GasPriceService<A, U> {
    latest_algorithm: Arc<RwLock<(BlockHeight, A)>>,
    update_algorithm: U,
}

impl<A, U> GasPriceService<A, U> {
    pub fn new(algorithm: Arc<RwLock<(BlockHeight, A)>>) -> Self {
        Self {
            latest_algorithm: algorithm,
        }
    }
}

#[cfg(test)]
mod tests {}
