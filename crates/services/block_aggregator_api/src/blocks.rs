use crate::result::Result;

pub trait BlockSource: Send + Sync {
    fn next_block(&mut self) -> impl Future<Output = Result<Block>> + Send;
}

#[derive(Clone)]
pub struct Block {
    bytes: Vec<u8>,
}

impl Block {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    #[cfg(test)]
    pub fn arb_size<Rng: rand::Rng + ?Sized>(rng: &mut Rng, size: usize) -> Self {
        let bytes: Vec<u8> = (0..size).map(|_| rng.random()).collect();
        Self::new(bytes)
    }

    #[cfg(test)]
    pub fn arb<Rng: rand::Rng + ?Sized>(rng: &mut Rng) -> Self {
        const SIZE: usize = 100;
        Self::arb_size(rng, SIZE)
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl From<Vec<u8>> for Block {
    fn from(bytes: Vec<u8>) -> Self {
        Self::new(bytes)
    }
}
