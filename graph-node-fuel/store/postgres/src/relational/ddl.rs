use std::{
    fmt::{self, Write},
    iter,
};

use graph::prelude::{BLOCK_NUMBER_MAX, ENV_VARS};

use crate::block_range::CAUSALITY_REGION_COLUMN;
use crate::relational::{
    ColumnType, BLOCK_COLUMN, BLOCK_RANGE_COLUMN, BYTE_ARRAY_PREFIX_SIZE, STRING_PREFIX_SIZE,
    VID_COLUMN,
};

use super::{Catalog, Column, Layout, SqlName, Table};

// In debug builds (for testing etc.) unconditionally create exclusion constraints, in release
// builds for production, skip them
#[cfg(debug_assertions)]
const CREATE_EXCLUSION_CONSTRAINT: bool = true;
#[cfg(not(debug_assertions))]
const CREATE_EXCLUSION_CONSTRAINT: bool = false;

impl Layout {
    /// Generate the DDL for the entire layout, i.e., all `create table`
    /// and `create index` etc. statements needed in the database schema
    ///
    /// See the unit tests at the end of this file for the actual DDL that
    /// gets generated
    pub fn as_ddl(&self) -> Result<String, fmt::Error> {
        let mut out = String::new();

        // Output enums first so table definitions can reference them
        self.write_enum_ddl(&mut out)?;

        // We sort tables here solely because the unit tests rely on
        // 'create table' statements appearing in a fixed order
        let mut tables = self.tables.values().collect::<Vec<_>>();
        tables.sort_by_key(|table| table.position);
        // Output 'create table' statements for all tables
        for table in tables {
            table.as_ddl(&self.catalog, &mut out)?;
        }

        Ok(out)
    }

    pub(crate) fn write_enum_ddl(&self, out: &mut dyn Write) -> Result<(), fmt::Error> {
        for name in self.input_schema.enum_types() {
            let values = self.input_schema.enum_values(name).unwrap();
            let mut sep = "";
            let name = SqlName::from(name);
            write!(
                out,
                "create type {}.{}\n    as enum (",
                self.catalog.site.namespace,
                name.quoted()
            )?;
            for value in values.iter() {
                write!(out, "{}'{}'", sep, value)?;
                sep = ", "
            }
            writeln!(out, ");")?;
        }
        Ok(())
    }
}

impl Table {
    /// Return an iterator over all the column names of this table
    ///
    // This needs to stay in sync with `create_table`
    pub(crate) fn column_names(&self) -> impl Iterator<Item = &str> {
        let block_column = if self.immutable {
            BLOCK_COLUMN
        } else {
            BLOCK_RANGE_COLUMN
        };
        let data_cols = self.columns.iter().map(|col| col.name.as_str());
        iter::once(VID_COLUMN)
            .chain(data_cols)
            .chain(iter::once(block_column))
    }

    // Changes to this function require changing `column_names`, too
    pub(crate) fn create_table(&self, out: &mut String) -> fmt::Result {
        fn columns_ddl(table: &Table) -> Result<String, fmt::Error> {
            let mut cols = String::new();
            let mut first = true;

            if table.has_causality_region {
                first = false;
                write!(
                    cols,
                    "{causality_region}     int not null",
                    causality_region = CAUSALITY_REGION_COLUMN
                )?;
            }

            for column in &table.columns {
                if !first {
                    writeln!(cols, ",")?;
                    write!(cols, "        ")?;
                }
                column.as_ddl(&mut cols)?;
                first = false;
            }

            Ok(cols)
        }

        if self.immutable {
            writeln!(
                out,
                "
    create table {qname} (
        {vid}                  bigserial primary key,
        {block}                int not null,\n\
        {cols},
        unique({id})
    );",
                qname = self.qualified_name,
                cols = columns_ddl(self)?,
                vid = VID_COLUMN,
                block = BLOCK_COLUMN,
                id = self.primary_key().name
            )
        } else {
            writeln!(
                out,
                r#"
    create table {qname} (
        {vid}                  bigserial primary key,
        {block_range}          int4range not null,
        {cols}
    );"#,
                qname = self.qualified_name,
                cols = columns_ddl(self)?,
                vid = VID_COLUMN,
                block_range = BLOCK_RANGE_COLUMN
            )?;

            self.exclusion_ddl(out)
        }
    }

    fn create_time_travel_indexes(&self, catalog: &Catalog, out: &mut String) -> fmt::Result {
        let (int4, int8) = catalog.minmax_ops();

        if self.immutable {
            write!(
                out,
                "create index brin_{table_name}\n    \
                on {qname}\n \
                   using brin({block} {int4}, vid {int8});\n",
                table_name = self.name,
                qname = self.qualified_name,
                block = BLOCK_COLUMN
            )
        } else {
            // Add a BRIN index on the block_range bounds to exploit the fact
            // that block ranges closely correlate with where in a table an
            // entity appears physically. This index is incredibly efficient for
            // reverts where we look for very recent blocks, so that this index
            // is highly selective. See https://github.com/graphprotocol/graph-node/issues/1415#issuecomment-630520713
            // for details on one experiment.
            //
            // We do not index the `block_range` as a whole, but rather the lower
            // and upper bound separately, since experimentation has shown that
            // Postgres will not use the index on `block_range` for clauses like
            // `block_range @> $block` but rather falls back to a full table scan.
            //
            // We also make sure that we do not put `NULL` in the index for
            // the upper bound since nulls can not be compared to anything and
            // will make the index less effective.
            //
            // To make the index usable, queries need to have clauses using
            // `lower(block_range)` and `coalesce(..)` verbatim.
            //
            // We also index `vid` as that correlates with the order in which
            // entities are stored.
            write!(out,"create index brin_{table_name}\n    \
                on {qname}\n \
                   using brin(lower(block_range) {int4}, coalesce(upper(block_range), {block_max}) {int4}, vid {int8});\n",
                table_name = self.name,
                qname = self.qualified_name,
                block_max = BLOCK_NUMBER_MAX)?;

            // Add a BTree index that helps with the `RevertClampQuery` by making
            // it faster to find entity versions that have been modified
            write!(
                out,
                "create index {table_name}_block_range_closed\n    \
                 on {qname}(coalesce(upper(block_range), {block_max}))\n \
                 where coalesce(upper(block_range), {block_max}) < {block_max};\n",
                table_name = self.name,
                qname = self.qualified_name,
                block_max = BLOCK_NUMBER_MAX
            )
        }
    }

    /// Calculates the indexing method and expression for a database column.
    ///
    /// ### Parameters
    /// * `immutable`: A boolean flag indicating whether the table is immutable.
    /// * `column`: A reference to the `Column` struct, representing the database column for which the index method and expression are being calculated.
    ///
    /// ### Returns
    /// A tuple `(String, String)` where:
    /// - The first element is the indexing method ("btree", "gist", or "gin"),
    /// - The second element is the index expression as a string.
    pub fn calculate_index_method_and_expression(
        immutable: bool,
        column: &Column,
    ) -> (String, String) {
        if column.is_reference() && !column.is_list() {
            if immutable {
                let index_expr = format!("{}, {}", column.name.quoted(), BLOCK_COLUMN);
                ("btree".to_string(), index_expr)
            } else {
                let index_expr = format!("{}, {}", column.name.quoted(), BLOCK_RANGE_COLUMN);
                ("gist".to_string(), index_expr)
            }
        } else {
            let index_expr = if column.use_prefix_comparison {
                match column.column_type {
                    ColumnType::String => {
                        format!("left({}, {})", column.name.quoted(), STRING_PREFIX_SIZE)
                    }
                    ColumnType::Bytes => format!(
                        "substring({}, 1, {})",
                        column.name.quoted(),
                        BYTE_ARRAY_PREFIX_SIZE
                    ),
                    // Handle other types if necessary, or maintain the unreachable statement
                    _ => unreachable!("only String and Bytes can have arbitrary size"),
                }
            } else {
                column.name.quoted()
            };

            let method = if column.is_list() || column.is_fulltext() {
                "gin".to_string()
            } else {
                "btree".to_string()
            };

            (method, index_expr)
        }
    }

    fn create_attribute_indexes(&self, out: &mut String) -> fmt::Result {
        // Create indexes.

        // Skip columns whose type is an array of enum, since there is no
        // good way to index them with Postgres 9.6. Once we move to
        // Postgres 11, we can enable that (tracked in graph-node issue
        // #1330)
        let not_enum_list = |col: &&Column| !(col.is_list() && col.is_enum());

        // We create a unique index on `id` in `create_table`
        // and don't need an explicit attribute index
        let not_immutable_pk = |col: &&Column| !(self.immutable && col.is_primary_key());

        // GIN indexes on numeric types are not very useful, but expensive
        // to build
        let not_numeric_list = |col: &&Column| {
            !(col.is_list()
                && [ColumnType::BigDecimal, ColumnType::BigInt, ColumnType::Int]
                    .contains(&col.column_type))
        };
        let columns = self
            .columns
            .iter()
            .filter(not_enum_list)
            .filter(not_immutable_pk)
            .filter(not_numeric_list);

        for (column_index, column) in columns.enumerate() {
            let (method, index_expr) =
                Self::calculate_index_method_and_expression(self.immutable, column);
            // If `create_gin_indexes` is set to false, we don't create
            // indexes on array attributes. Experience has shown that these
            // indexes are very expensive to update and can have a very bad
            // impact on the write performance of the database, but are
            // hardly ever used or needed by queries.
            if !column.is_list() || ENV_VARS.store.create_gin_indexes {
                write!(
                    out,
                    "create index attr_{table_index}_{column_index}_{table_name}_{column_name}\n    on {qname} using {method}({index_expr});\n",
                    table_index = self.position,
                    table_name = self.name,
                    column_name = column.name,
                    qname = self.qualified_name,
                )?;
            }
        }
        writeln!(out)
    }

    /// Generate the DDL for one table, i.e. one `create table` statement
    /// and all `create index` statements for the table's columns
    ///
    /// See the unit tests at the end of this file for the actual DDL that
    /// gets generated
    pub(crate) fn as_ddl(&self, catalog: &Catalog, out: &mut String) -> fmt::Result {
        self.create_table(out)?;
        self.create_time_travel_indexes(catalog, out)?;
        self.create_attribute_indexes(out)
    }

    pub fn exclusion_ddl(&self, out: &mut String) -> fmt::Result {
        // Tables with causality regions need to use exclusion constraints for correctness,
        // to catch violations of write isolation.
        let as_constraint = self.has_causality_region || CREATE_EXCLUSION_CONSTRAINT;

        self.exclusion_ddl_inner(out, as_constraint)
    }

    // `pub` for tests.
    pub(crate) fn exclusion_ddl_inner(&self, out: &mut String, as_constraint: bool) -> fmt::Result {
        if as_constraint {
            writeln!(
                out,
                "
    alter table {qname}
        add constraint {bare_name}_{id}_{block_range}_excl exclude using gist ({id} with =, {block_range} with &&);",
                qname = self.qualified_name,
                bare_name = self.name,
                id = self.primary_key().name,
                block_range = BLOCK_RANGE_COLUMN
            )?;
        } else {
            writeln!(
                out,
                "
        create index {bare_name}_{id}_{block_range}_excl on {qname}
         using gist ({id}, {block_range});
               ",
                qname = self.qualified_name,
                bare_name = self.name,
                id = self.primary_key().name,
                block_range = BLOCK_RANGE_COLUMN
            )?;
        }
        Ok(())
    }
}

impl Column {
    /// Generate the DDL for one column, i.e. the part of a `create table`
    /// statement for this column.
    ///
    /// See the unit tests at the end of this file for the actual DDL that
    /// gets generated
    fn as_ddl(&self, out: &mut String) -> fmt::Result {
        write!(out, "{:20} {}", self.name.quoted(), self.sql_type())?;
        if self.is_list() {
            write!(out, "[]")?;
        }
        if self.is_primary_key() || !self.is_nullable() {
            write!(out, " not null")?;
        }
        Ok(())
    }
}
