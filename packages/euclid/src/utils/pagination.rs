use cosmwasm_schema::cw_serde;

#[cw_serde]
pub struct Pagination<T> {
    pub min: Option<T>,
    pub max: Option<T>,
    pub skip: Option<u64>,
    pub limit: Option<u64>,
}
pub const DEFAULT_PAGINATION_LIMIT: u64 = 10;
pub const DEFAULT_PAGINATION_SKIP: u64 = 0;

impl<T: ToString> Pagination<T> {
    // Creates a new instance of Pagination
    pub fn new(min: Option<T>, max: Option<T>, skip: Option<u64>, limit: Option<u64>) -> Self {
        Pagination {
            min,
            max,
            skip,
            limit,
        }
    }
}
