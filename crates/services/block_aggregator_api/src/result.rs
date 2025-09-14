#[derive(Debug)]
pub enum Error {
    ApiError,
    BlockSourceError,
}
pub type Result<T, E = Error> = core::result::Result<T, E>;
