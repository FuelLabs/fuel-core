/// Specifies the direction of a paginated query
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PageDirection {
    Forward,
    Backward,
}

/// Used to parameterize paginated queries
#[derive(Clone, Debug)]
pub struct PaginationRequest<T> {
    /// The cursor returned from a previous query to indicate an offset
    pub cursor: Option<T>,
    /// The number of results to take
    pub results: i32,
    /// The direction of the query (e.g. asc, desc order).
    pub direction: PageDirection,
}

pub struct PaginatedResult<T, C> {
    pub cursor: Option<C>,
    pub results: Vec<T>,
    pub has_next_page: bool,
    pub has_previous_page: bool,
}
