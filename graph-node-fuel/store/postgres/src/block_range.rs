use diesel::pg::Pg;
use diesel::query_builder::{AstPass, QueryFragment};
use diesel::result::QueryResult;
///! Utilities to deal with block numbers and block ranges
use diesel::serialize::{Output, ToSql};
use diesel::sql_types::{Integer, Range};
use graph::env::ENV_VARS;
use std::io::Write;
use std::ops::{Bound, RangeBounds, RangeFrom};

use graph::prelude::{lazy_static, BlockNumber, BlockPtr, BLOCK_NUMBER_MAX};

use crate::relational::{SqlName, Table};

/// The name of the column in which we store the block range for mutable
/// entities
pub(crate) const BLOCK_RANGE_COLUMN: &str = "block_range";

/// The name of the column that stores the causality region of an entity.
pub(crate) const CAUSALITY_REGION_COLUMN: &str = "causality_region";

/// The SQL clause we use to check that an entity version is current;
/// that version has an unbounded block range, but checking for
/// `upper_inf(block_range)` is slow and can't use the exclusion
/// index we have on entity tables; we therefore check if i32::MAX is
/// in the range
pub(crate) const BLOCK_RANGE_CURRENT: &str = "block_range @> 2147483647";

/// Most subgraph metadata entities are not versioned. For such entities, we
/// want two things:
///   - any CRUD operation modifies such an entity in place
///   - queries by a block number consider such an entity as present for
///     any block number
/// We therefore mark such entities with a block range `[-1,\infinity)`; we
/// use `-1` as the lower bound to make it easier to identify such entities
/// for troubleshooting/debugging
pub(crate) const BLOCK_UNVERSIONED: i32 = -1;

pub(crate) const UNVERSIONED_RANGE: (Bound<i32>, Bound<i32>) =
    (Bound::Included(BLOCK_UNVERSIONED), Bound::Unbounded);

/// The name of the column in which we store the block from which an
/// immutable entity is visible
pub(crate) const BLOCK_COLUMN: &str = "block$";

lazy_static! {
    pub(crate) static ref BLOCK_RANGE_COLUMN_SQL: SqlName =
        SqlName::verbatim(BLOCK_RANGE_COLUMN.to_string());
    pub(crate) static ref BLOCK_COLUMN_SQL: SqlName = SqlName::verbatim(BLOCK_COLUMN.to_string());
}

/// The range of blocks for which an entity is valid. We need this struct
/// to bind ranges into Diesel queries.
#[derive(Clone, Debug)]
pub struct BlockRange(Bound<BlockNumber>, Bound<BlockNumber>);

pub(crate) fn first_block_in_range(
    bound: &(Bound<BlockNumber>, Bound<BlockNumber>),
) -> Option<BlockNumber> {
    if bound == &UNVERSIONED_RANGE {
        return None;
    }

    match bound.0 {
        Bound::Included(nr) => Some(nr),
        Bound::Excluded(nr) => Some(nr + 1),
        Bound::Unbounded => None,
    }
}

/// Return the block number contained in the history event. If it is
/// `None` panic because that indicates that we want to perform an
/// operation that does not record history, which should not happen
/// with how we currently use relational schemas
pub(crate) fn block_number(block_ptr: &BlockPtr) -> BlockNumber {
    block_ptr.number
}

impl From<RangeFrom<BlockNumber>> for BlockRange {
    fn from(range: RangeFrom<BlockNumber>) -> BlockRange {
        BlockRange(range.start_bound().cloned(), range.end_bound().cloned())
    }
}

impl From<std::ops::Range<BlockNumber>> for BlockRange {
    fn from(range: std::ops::Range<BlockNumber>) -> BlockRange {
        BlockRange(Bound::Included(range.start), Bound::Excluded(range.end))
    }
}

impl ToSql<Range<Integer>, Pg> for BlockRange {
    fn to_sql<W: Write>(&self, out: &mut Output<W, Pg>) -> diesel::serialize::Result {
        let pair = (self.0, self.1);
        ToSql::<Range<Integer>, Pg>::to_sql(&pair, out)
    }
}

#[derive(Constructor)]
pub struct BlockRangeLowerBoundClause<'a> {
    _table_prefix: &'a str,
    block: BlockNumber,
}

impl<'a> QueryFragment<Pg> for BlockRangeLowerBoundClause<'a> {
    fn walk_ast(&self, mut out: AstPass<Pg>) -> QueryResult<()> {
        out.unsafe_to_cache_prepared();

        out.push_sql("lower(");
        out.push_identifier(BLOCK_RANGE_COLUMN)?;
        out.push_sql(") = ");
        out.push_bind_param::<Integer, _>(&self.block)?;

        Ok(())
    }
}

#[derive(Constructor)]
pub struct BlockRangeUpperBoundClause<'a> {
    _table_prefix: &'a str,
    block: BlockNumber,
}

impl<'a> QueryFragment<Pg> for BlockRangeUpperBoundClause<'a> {
    fn walk_ast(&self, mut out: AstPass<Pg>) -> QueryResult<()> {
        out.unsafe_to_cache_prepared();

        out.push_sql("coalesce(upper(");
        out.push_identifier(BLOCK_RANGE_COLUMN)?;
        out.push_sql("), 2147483647) = ");
        out.push_bind_param::<Integer, _>(&self.block)?;

        Ok(())
    }
}

/// Helper for generating various SQL fragments for handling the block range
/// of entity versions
#[derive(Debug, Clone, Copy)]
pub enum BlockRangeColumn<'a> {
    Mutable {
        table: &'a Table,
        table_prefix: &'a str,
        block: BlockNumber,
    },
    Immutable {
        table: &'a Table,
        table_prefix: &'a str,
        block: BlockNumber,
    },
}

impl<'a> BlockRangeColumn<'a> {
    pub fn new(table: &'a Table, table_prefix: &'a str, block: BlockNumber) -> Self {
        if table.immutable {
            Self::Immutable {
                table,
                table_prefix,
                block,
            }
        } else {
            Self::Mutable {
                table,
                table_prefix,
                block,
            }
        }
    }
}

impl<'a> BlockRangeColumn<'a> {
    /// Output SQL that matches only rows whose block range contains `block`.
    ///
    /// `filters_by_id` has no impact on correctness. It is a heuristic to determine
    /// whether the brin index should be used. If `true`, the brin index is not used.
    pub fn contains(&self, out: &mut AstPass<Pg>, filters_by_id: bool) -> QueryResult<()> {
        out.unsafe_to_cache_prepared();

        match self {
            BlockRangeColumn::Mutable { table, block, .. } => {
                self.name(out);
                out.push_sql(" @> ");
                out.push_bind_param::<Integer, _>(block)?;

                let should_use_brin = !filters_by_id || ENV_VARS.store.use_brin_for_all_query_types;
                if table.is_account_like && *block < BLOCK_NUMBER_MAX && should_use_brin {
                    // When block is BLOCK_NUMBER_MAX, these checks would be wrong; we
                    // don't worry about adding the equivalent in that case since
                    // we generally only see BLOCK_NUMBER_MAX here for metadata
                    // queries where block ranges don't matter anyway.
                    //
                    // We also don't need to add these if the query already filters by ID,
                    // because the ideal index is the GiST index on id and block_range.
                    out.push_sql(" and coalesce(upper(");
                    out.push_identifier(BLOCK_RANGE_COLUMN)?;
                    out.push_sql("), 2147483647) > ");
                    out.push_bind_param::<Integer, _>(block)?;
                    out.push_sql(" and lower(");
                    out.push_identifier(BLOCK_RANGE_COLUMN)?;
                    out.push_sql(") <= ");
                    out.push_bind_param::<Integer, _>(block)
                } else {
                    Ok(())
                }
            }
            BlockRangeColumn::Immutable { block, .. } => {
                if *block == BLOCK_NUMBER_MAX {
                    // `self.block <= BLOCK_NUMBER_MAX` is always true
                    out.push_sql("true");
                    Ok(())
                } else {
                    self.name(out);
                    out.push_sql(" <= ");
                    out.push_bind_param::<Integer, _>(block)
                }
            }
        }
    }

    pub fn column_name(&self) -> &str {
        match self {
            BlockRangeColumn::Mutable { .. } => BLOCK_RANGE_COLUMN,
            BlockRangeColumn::Immutable { .. } => BLOCK_COLUMN,
        }
    }

    /// Output the qualified name of the block range column
    pub fn name(&self, out: &mut AstPass<Pg>) {
        match self {
            BlockRangeColumn::Mutable { table_prefix, .. } => {
                out.push_sql(table_prefix);
                out.push_sql(BLOCK_RANGE_COLUMN);
            }
            BlockRangeColumn::Immutable { table_prefix, .. } => {
                out.push_sql(table_prefix);
                out.push_sql(BLOCK_COLUMN);
            }
        }
    }

    /// Output an expression that matches rows that are the latest version
    /// of their entity
    pub fn latest(&self, out: &mut AstPass<Pg>) {
        match self {
            BlockRangeColumn::Mutable { .. } => out.push_sql(BLOCK_RANGE_CURRENT),
            BlockRangeColumn::Immutable { .. } => out.push_sql("true"),
        }
    }

    /// Output SQL that updates the block range column so that the row is
    /// only valid up to `block` (exclusive)
    ///
    /// # Panics
    ///
    /// If the underlying table is immutable, this method will panic
    pub fn clamp(&self, out: &mut AstPass<Pg>) -> QueryResult<()> {
        match self {
            BlockRangeColumn::Mutable { block, .. } => {
                self.name(out);
                out.push_sql(" = int4range(lower(");
                out.push_identifier(BLOCK_RANGE_COLUMN)?;
                out.push_sql("), ");
                out.push_bind_param::<Integer, _>(block)?;
                out.push_sql(")");
                Ok(())
            }
            BlockRangeColumn::Immutable { .. } => {
                unreachable!("immutable entities can not be updated or deleted")
            }
        }
    }

    /// Output the name of the block range column without the table prefix
    pub(crate) fn bare_name(&self, out: &mut AstPass<Pg>) {
        match self {
            BlockRangeColumn::Mutable { .. } => out.push_sql(BLOCK_RANGE_COLUMN),
            BlockRangeColumn::Immutable { .. } => out.push_sql(BLOCK_COLUMN),
        }
    }

    /// Output an expression that matches all rows that have been changed
    /// after `block` (inclusive)
    pub(crate) fn changed_since(&self, out: &mut AstPass<Pg>) -> QueryResult<()> {
        match self {
            BlockRangeColumn::Mutable { block, .. } => {
                out.push_sql("lower(");
                out.push_identifier(BLOCK_RANGE_COLUMN)?;
                out.push_sql(") >= ");
                out.push_bind_param::<Integer, _>(block)
            }
            BlockRangeColumn::Immutable { block, .. } => {
                out.push_identifier(BLOCK_COLUMN)?;
                out.push_sql(" >= ");
                out.push_bind_param::<Integer, _>(block)
            }
        }
    }
}

#[test]
fn block_number_max_is_i32_max() {
    // The code in this file embeds i32::MAX aka BLOCK_NUMBER_MAX in strings
    // for efficiency. This assertion makes sure that BLOCK_NUMBER_MAX still
    // is what we think it is
    assert_eq!(2147483647, BLOCK_NUMBER_MAX);
}
