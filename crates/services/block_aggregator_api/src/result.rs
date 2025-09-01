#[derive(Debug)]
pub enum Error {
    ApiError,
}
pub type Result<T, E = Error> = core::result::Result<T, E>;
