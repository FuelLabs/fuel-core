# Time-travel queries

Time-travel queries make it possible to query the state of a subgraph at
a given block. Assume that a subgraph has an `Account` entity defined
as

```graphql
type Account @entity {
    id ID!
    balance BigInt
}
```

The corresponding database table for that will have the form
```sql
create table account(
    vid     int8 primary key,
    id      text not null,
    balance numeric,
    block_range int4range not null,
    exclude using gist(id with =, block_range with &&)
);
```

The `account` table will contain one entry for each version of each account;
that means that for the account with `id = "1"`, there will be multiple rows
in the database, but with different block ranges. The exclusion constraint
makes sure that the block ranges for any given entity do not overlap.

The block range indicates from which block (inclusive) to which block
(exclusive) an entity version is valid. The most recent version of an entity
has a block range with an unlimited upper bound. The block range `[7, 15)`
means that this version of the entity should be used for queries that want
to know the state of the account if the query asks for block heights between
7 and 14.

A nice benefit of this approach is that we do not modify the data for
existing entities. The only attribute of an entity that can ever be modified
is the range of blocks for which a specific entity version is valid. This
will become particularly significant once zHeap is fully integrated into
Postgres (anticipated for Postgres 14)

For background on ranges in Postgres, see the
[rangetypes](https://www.postgresql.org/docs/9.6/rangetypes.html) and
[range operators](https://www.postgresql.org/docs/9.6/functions-range.html)
chapters in the documentation.

### Immutable entities

For entity types declared with `@entity(immutable: true)`, the table has a
`block$ int not null` column instead of a `block_range` column, where the
`block$` column stores the value that would be stored in
`lower(block_range)`. Since the upper bound of the block range for an
immutable entity is always infinite, a test like `block_range @> $B`, which
is equivalent to `lower(block_range) <= $B and upper(block_range) > $B`,
can be simplified to `block$ <= $B`.

The operations in the next section are adjusted accordingly for immutable
entities.

## Operations

For all operations, we assume that we perform them for block number `B`;
for most of them we only focus on how the `block_range` is used and
manipulated. The values for omitted columns should be clear from context.

For deletion and update, we only modify the current version,

### Querying for a point-in-time

Any query that selects entities will have a condition added to it that
requires that the `block_range` for the entity must include `B`:

```sql
    select .. from account
     where ..
       and block_range @> $B
```

### Create entity

Creating an entity consist of writing an entry with a block range marking
it valid from `B` to infinity:

```sql
    insert into account(id, block_range, ...)
    values ($ID, '[$B,]', ...);
```

### Delete entity

Only the current version of an entity can be deleted. For that version,
deleting it consists of clamping the block range at `B`:

```sql
    update account
       set block_range = int4range(lower(block_range), $B)
     where id = $ID and block_range @> $INTMAX;
```

Note that this operation is not allowed for immutable entities.

### Update entity

Only the current version of an entity can be updated. An update is performed
as a deletion followed by an insertion.

Note that this operation is not allowed for immutable entities.

### Rolling back

When we need to revert entity changes that happened for blocks with numbers
higher than `B`, we delete all entities which would only be valid 'in the
future', and then open the range of the one entity entry for which the
range contains `B`, thereby marking it as the current version:

```sql
    delete from account lower(block_range) >= $B;

    update account
       set block_range = int4range(lower(block_range), NULL)
     where block_range @> $B;
```

## Notes

- It is important to note that the block number does not uniquely identify a
  block, only the block hash does. But within our database, at any given
  moment in time, we can identify the block for a given block number and
  subgraph by following the chain starting at the subgraph's block pointer
  back. In practice, the query to do that is expensive for blocks far away
  from the subgraph's head block, but so far we have not had a need for
  that.
