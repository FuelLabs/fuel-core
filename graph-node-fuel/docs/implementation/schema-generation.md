# Schema Generation

This document describes how we go from a GraphQL schema to a relational
table definition in Postgres.

Schema generation follows a few simple rules:

* the data for a subgraph is entirely stored in a Postgres namespace whose
  name is `sgdNNNN`. The mapping between namespace name and deployment id is
  kept in `deployment_schemas`
* the data for each entity type is stored in a table whose structure follows
  the declaration of the type in the GraphQL schema
* enums in the GraphQL schema are stored as enum types in Postgres
* interfaces are not stored in the database, only the concrete types that
  implement the interface are stored

Any table for an entity type has the following structure:

```sql
    create table sgd42.account(
        vid int8    serial primary key,
        id          text not null, -- or bytea
        .. attributes ..
        block_range int4range not null
    )
```

The `vid` is used in some situations to uniquely identify the specific
version of an entity. The `block_range` is used to enable [time-travel
queries](./time-travel.md).

The attributes of the GraphQL type correspond directly to columns in the
generated table. The types of these columns are

* the `id` column can have type `ID`, `String`, and `Bytes`, where `ID` is
  an alias for `String` for historical reasons.
* if the attribute has a primitive type, the column has the SQL type that
  most closely mirrors the GraphQL type. `BigDecimal` and `BigInt` are
  stored as `numeric`, `Bytes` is stored as `bytea`, etc.
* if the attribute references another entity, the column has the type of the
  `id` type of the referenced entity type. We do not use foreign key
  constraints to allow storing an entity that references an entity that will
  only be created later. Foreign key constraint violations will therefore
  only be detected when a query is issued, or simply lead to the reference
  missing from the query result.
* if the attribute has an enum type, we generate a SQL enum type and use
  that as the type of the column.
* if the attribute has a list type, like `[String]`, the corresponding
  column uses an array type. We do not allow nested arrays like `[[String]]`
  in GraphQL, so arrays will only ever contain entries of a primitive type.

### Immutable entities

Entity types declared with a plain `@entity` in the GraphQL schema are
mutable, and the above table design enables selecting one of many versions
of the same entity, depending on the block height at which the query is
run. In a lot of cases, the subgraph author knows that entities will never
be mutated, e.g., because they are just a direct copy of immutable chain data,
like a transfer. In those cases, we know that the upper end of the block
range will always be infinite and don't need to store that explicitly.

When an entity type is declared with `@entity(immutable: true)` in the
GraphQL schema, we do not generate a `block_range` column in the
corresponding table. Instead, we generate a column `block$ int not null`,
so that the check whether a row is visible at block `B` simplifies to
`block$ <= B`.

Furthermore, since each entity can only have one version, we also add a
constraint `unique(id)` to such tables, and can avoid expensive GiST
indexes in favor of simple BTree indexes since the `block$` column is an
integer.

## Indexing

We do not know ahead of time which queries will be issued and therefore
build indexes extensively. This leads to serious overindexing, but both
reducing the overindexing and making it possible to generate custom indexes
are open issues at this time.

We generate the following indexes for each table:

* for mutable entity types
  * an exclusion index over `(id, block_range)` that ensures that the
    versions for the same entity `id` have disjoint block ranges
  * a BRIN index on `(lower(block_range), COALESCE(upper(block_range),
    2147483647), vid)` that helps speed up some operations, especially
    reversion, in tables that have good data locality, for example, tables
    where entities are never updated or deleted
* for immutable entity types
  * a unique index on `id`
  * a BRIN index on `(block$, vid)`
* for each attribute, an index called `attr_N_M_..` where `N` is the number
  of the entity type in the GraphQL schema, and `M` is the number of the
  attribute within that type. For attributes of a primitive type, the index
  is a BTree index. For attributes that reference other entities, the index
  is a GiST index on `(attribute, block_range)`

### Indexes on String Attributes

In some cases, `String` attributes are used to store large pieces of text,
text that is longer than the limit that Postgres imposes on individual index
entries. For such attributes, we therefore index `left(attribute,
STRING_PREFIX_SIZE)`. When we generate queries, query generation makes sure
that this index is usable by adding additional clauses to the query that use
`left(attribute, STRING_PREFIX_SIZE)` in the query. For example, if a query
was looking for entities where the `name` equals `"Hamming"`, the query
would contain a clause `left(name, STRING_PREFIX_SIZE) = 'Hamming'`.

## Known Issues

- Storing arrays as array attributes in Postgres can have catastrophically
  bad performance if the size of the array is not bounded by a relatively
  small number.
- Overindexing leads to large amounts of storage used for indexes, and, of
  course, slows down writes.
- At the same time, indexes are not always usable. For example, a BTree
  index on `name` is not usable for sorting entities, since we always add
  `id` to the `order by` clause, i.e., when a user asks for entities ordered
  by `name`, we actually include `order by name, id` in the SQL query to
  guarantee an unambiguous ordering. Incremental sorting in Postgres 13
  might help with that.
- Lack of support for custom indexes makes it hard to transfer manually
  created indexes between different versions of the same subgraph. By
  convention, manually created indexes should have a name that starts with
  `manual_`.
