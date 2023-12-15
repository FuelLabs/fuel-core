use diesel::{
    pg::Pg,
    serialize::Output,
    sql_types::Integer,
    types::{FromSql, ToSql},
    FromSqlRow,
};
use std::fmt;
use std::io;

use crate::components::subgraph::Entity;

/// The causality region of a data source. All onchain data sources share the same causality region,
/// but each offchain data source is assigned its own. This isolates offchain data sources from
/// onchain and from each other.
///
/// The isolation rules are:
/// 1. A data source cannot read an entity from a different causality region.
/// 2. A data source cannot update or overwrite an entity from a different causality region.
///
/// This necessary for determinism because offchain data sources don't have a deterministic order of
/// execution, for example an IPFS file may become available at any point in time. The isolation
/// rules make the indexing result reproducible, given a set of available files.
#[derive(Debug, Copy, Clone, PartialEq, Eq, FromSqlRow, Hash, PartialOrd, Ord)]
pub struct CausalityRegion(i32);

impl fmt::Display for CausalityRegion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromSql<Integer, Pg> for CausalityRegion {
    fn from_sql(bytes: Option<&[u8]>) -> diesel::deserialize::Result<Self> {
        <i32 as FromSql<Integer, Pg>>::from_sql(bytes).map(CausalityRegion)
    }
}

impl ToSql<Integer, Pg> for CausalityRegion {
    fn to_sql<W: io::Write>(&self, out: &mut Output<W, Pg>) -> diesel::serialize::Result {
        <i32 as ToSql<Integer, Pg>>::to_sql(&self.0, out)
    }
}

impl CausalityRegion {
    /// The causality region of all onchain data sources.
    pub const ONCHAIN: CausalityRegion = CausalityRegion(0);

    pub const fn next(self) -> Self {
        CausalityRegion(self.0 + 1)
    }

    pub fn from_entity(entity: &Entity) -> Self {
        entity
            .get("causality_region")
            .and_then(|v| v.as_int())
            .map(CausalityRegion)
            .unwrap_or(CausalityRegion::ONCHAIN)
    }
}

/// A subgraph will assign causality regions to offchain data sources from a sequence.
pub struct CausalityRegionSeq(pub CausalityRegion);

impl CausalityRegionSeq {
    /// Create a new sequence with the current value set to `ONCHAIN`, which is 0, therefore the
    /// first produced value will be `ONCHAIN + 1`, which is 1.
    const fn new() -> Self {
        CausalityRegionSeq(CausalityRegion::ONCHAIN)
    }

    /// A sequence with the current value set to `cr`. If `cr` is `None`, then the current value is
    /// set to `ONCHAIN`, which is 0. The next produced value will be `cr + 1`.
    pub fn from_current(cr: Option<CausalityRegion>) -> CausalityRegionSeq {
        cr.map(CausalityRegionSeq)
            .unwrap_or(CausalityRegionSeq::new())
    }

    pub fn next_val(&mut self) -> CausalityRegion {
        self.0 = self.0.next();
        self.0
    }
}
