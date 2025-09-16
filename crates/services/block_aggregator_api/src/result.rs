#[derive(Debug)]
pub enum Error {
    Api,
    BlockSource,
    DB(anyhow::Error),
}

pub type Result<T, E = Error> = core::result::Result<T, E>;
