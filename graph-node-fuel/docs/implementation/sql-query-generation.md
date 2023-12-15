## SQL Query Generation

### Goal

For a GraphQL query of the form

```graphql
query {
  parents(filter) {
    id
    children(filter) {
      id
    }
  }
}
```

we want to generate only two SQL queries: one to get the parents, and one to
get the children for all those parents. The fact that `children` is nested
under `parents` requires that we add a filter to the `children` query that
restricts children to those that are related to the parents we fetched in
the first query to get the parents. How exactly we filter the `children`
query depends on how the relationship between parents and children is
modeled in the GraphQL schema, and on whether one (or both) of the types
involved are interfaces.

The rest of this writeup is concerned with how to generate the query for
`children`, assuming we already retrieved the list of all parents.

The bulk of the implementation of this feature can be found in
`graphql/src/store/prefetch.rs`, `store/postgres/src/relational.rs`, and
`store/postgres/src/relational_queries.rs`


### Handling first/skip

We never get all the `children` for a parent; instead we always have a
`first` and `skip` argument in the children filter. Those arguments need to
be applied to each parent individually by ranking the children for each
parent according to the order defined by the `children` query. If the same
child matches multiple parents, we need to make sure that it is considered
separately for each parent as it might appear at different ranks for
different parents. In SQL, we use a lateral join,  essentially a for loop.
For children that store the id of their parent in `parent_id`, we'd run the
following query:

```sql
select c.*, p.id
  from unnest({parent_ids}) as p(id)
        cross join lateral
         (select *
            from children c
           where c.parent_id = p.id
             and .. other conditions on c ..
           order by c.{sort_key}, c.id
           limit {first}
          offset {skip}) c
 order by p.id, c.{sort_key}, c.id
```

Note that we order children by the sort key the user specified, followed by
the `id` to guarantee an unambiguous order even if the sort key is a
non-unique column. Unfortunately, we do not know which attributes of an
entity are unique and which ones aren't.

### Handling parent/child relationships

How we get the children for a set of parents depends on how the relationship
between the two is modeled. The interesting parameters there are whether
parents store a list or a single child, and whether that field is derived,
together with the same for children.

There are a total of 16 combinations of these four boolean variables; four
of them, when both parent and child derive their fields, are not
permissible. It also doesn't matter whether the child derives its parent
field: when the parent field is not derived, we need to use that since that
is the only place that contains the parent -> child relationship. When the
parent field is derived, the child field can not be a derived field.

That leaves us with eight combinations of whether the parent and child store
a list or a scalar value, and whether the parent is derived. For details on
the GraphQL schema for each row in this table, see the section at the end.
The `Join cond` indicates how we can find the children for a given parent.
The table refers to the four different kinds of join condition we might need
as types A, B, C, and D.

| Case | Parent list? | Parent derived? | Child list? | Join cond                  | Type |
|------|--------------|-----------------|-------------|----------------------------|------|
|    1 | TRUE         | TRUE            | TRUE        | child.parents ∋ parent.id  | A    |
|    2 | FALSE        | TRUE            | TRUE        | child.parents ∋ parent.id  | A    |
|    3 | TRUE         | TRUE            | FALSE       | child.parent = parent.id   | B    |
|    4 | FALSE        | TRUE            | FALSE       | child.parent = parent.id   | B    |
|    5 | TRUE         | FALSE           | TRUE        | child.id ∈ parent.children | C    |
|    6 | TRUE         | FALSE           | FALSE       | child.id ∈ parent.children | C    |
|    7 | FALSE        | FALSE           | TRUE        | child.id = parent.child    | D    |
|    8 | FALSE        | FALSE           | FALSE       | child.id = parent.child    | D    |

In addition to how the data about the parent/child relationship is stored,
the multiplicity of the parent/child relationship also influences query
generation: if each parent can have at most a single child, queries can be
much simpler than if we have to account for multiple children per parent,
which requires paginating them. We also need to detect cases where the
mappings created multiple children per parent. We do this by adding a clause
`limit {parent_ids.len} + 1` to the query, so that if there is one parent
with multiple children, we will select it, but still protect ourselves
against mappings that produce catastrophically bad data with huge numbers of
children per parent. The GraphQL execution logic will detect that there is a
parent with multiple children, and generate an error.

When we query children, we already have a list of all parents from running a
previous query. To find the children, we need to have the id of the parent
that child is related to, and, when the parent stores the ids of its
children directly (types C and D) the child ids for each parent id.

The following queries all produce a relation that has the same columns as
the table holding children, plus a column holding the id of the parent that
the child belongs to.

#### Type A

Use when child stores a list of parents

Data needed to generate:

-   children: name of child table
-   parent_ids: list of parent ids
-   parent_field: name of parents field (array) in child table
-   single: boolean to indicate whether a parent has at most one child or
    not

The implementation uses an `EntityLink::Direct` for joins of this type.

##### Multiple children per parent
```sql
select c.*, p.id as parent_id
  from unnest({parent_ids}) as p(id)
       cross join lateral
       (select *
          from children c
         where p.id = any(c.{parent_field})
           and .. other conditions on c ..
         order by c.{sort_key}
         limit {first} offset {skip}) c
 order by c.{sort_key}
```

##### Single child per parent
```sql
select c.*, p.id as parent_id
  from unnest({parent_ids}) as p(id),
       children c
 where c.{parent_field} @> array[p.id]
   and .. other conditions on c ..
 limit {parent_ids.len} + 1
```

#### Type B

Use when child stores a single parent

Data needed to generate:

-   children: name of child table
-   parent_ids: list of parent ids
-   parent_field: name of parent field (scalar) in child table
-   single: boolean to indicate whether a parent has at most one child or
    not

The implementation uses an `EntityLink::Direct` for joins of this type.

##### Multiple children per parent
```sql
select c.*, p.id as parent_id
  from unnest({parent_ids}) as p(id)
       cross join lateral
       (select *
          from children c
         where p.id = c.{parent_field}
           and .. other conditions on c ..
         order by c.{sort_key}
         limit {first} offset {skip}) c
 order by c.{sort_key}
```

##### Single child per parent

```sql
select c.*, c.{parent_field} as parent_id
  from children c
 where c.{parent_field} = any({parent_ids})
   and .. other conditions on c ..
 limit {parent_ids.len} + 1
```

Alternatively, this is worth a try, too:
```sql
select c.*, c.{parent_field} as parent_id
  from unnest({parent_ids}) as p(id), children c
 where c.{parent_field} = p.id
   and .. other conditions on c ..
 limit {parent_ids.len} + 1
```

#### Type C

Use when the parent stores a list of its children.

Data needed to generate:

-   children: name of child table
-   parent_ids: list of parent ids
-   child\_id_matrix: array of arrays where `child_id_matrix[i]` is an array
    containing the ids of the children for `parent_id[i]`

The implementation uses a `EntityLink::Parent` for joins of this type.

##### Multiple children per parent

```sql
select c.*, p.id as parent_id
  from rows from (unnest({parent_ids}), reduce_dim({child_id_matrix}))
              as p(id, child_ids)
       cross join lateral
       (select *
          from children c
         where c.id = any(p.child_ids)
           and .. other conditions on c ..
         order by c.{sort_key}
         limit {first} offset {skip}) c
 order by c.{sort_key}
```

Note that `reduce_dim` is a custom function that is not part of [ANSI
SQL:2016](https://en.wikipedia.org/wiki/SQL:2016) but is needed as there is
no standard way to decompose a matrix into a table where each row contains
one row of the matrix. The `ROWS FROM` construct is also not part of ANSI
SQL.

##### Single child per parent

Not possible with relations of this type

#### Type D

Use when parent is not a list and not derived

Data needed to generate:

-   children: name of child table
-   parent_ids: list of parent ids
-   child_ids: list of the id of the child for each parent such that
    `child_ids[i]` is the id of the child for `parent_id[i]`

The implementation uses a `EntityLink::Parent` for joins of this type.

##### Multiple children per parent

Not possible with relations of this type

##### Single child per parent

```sql
select c.*, p.id as parent_id
  from rows from (unnest({parent_ids}), unnest({child_ids})) as p(id, child_id),
       children c
 where c.id = p.child_id
   and .. other conditions on c ..
```

If the list of unique `child_ids` is small enough, we also add a where
clause `c.id = any({ unique child_ids })`. The list is small enough if it
contains fewer than `TYPED_CHILDREN_SET_SIZE` (default: 150) unique child
ids.


The `ROWS FROM` construct is not part of ANSI SQL.

### Handling interfaces

If the GraphQL type of the children is an interface, we need to take
special care to form correct queries. Whether the parents are
implementations of an interface or not does not matter, as we will have a
full list of parents already loaded into memory when we build the query for
the children. Whether the GraphQL type of the parents is an interface may
influence from which parent attribute we get child ids for queries of type
C and D.

When the GraphQL type of the children is an interface, we resolve the
interface type into the concrete types implementing it, produce a query for
each concrete child type and combine those queries via `union all`.

Since implementations of the same interface will generally differ in the
schema they use, we can not form a `union all` of all the data in the
tables for these concrete types, but have to first query only attributes
that we know will be common to all entities implementing the interface,
most notably the `vid` (a unique identifier that identifies the precise
version of an entity), and then later fill in the details of each entity by
converting it directly to JSON. A second reason to pass entities as JSON
from the database is that it is impossible with Diesel to execute queries
where the number and types of the columns of the result are not known at
compile time.

We need to to be careful though to not convert to JSONB too early, as that
is slow when done for large numbers of rows. Deferring conversion is
responsible for some of the complexity in these queries.

That means that when we deal with children that are an interface, we will
first select only the following columns from each concrete child type
(where exactly they come from depends on how the parent/child relationship
is modeled)

```sql
select '{__typename}' as entity, c.vid, c.id, c.{sort_key}, p.id as parent_id
```

and then use that data to fill in the complete details of each concrete
entity. The query `type_query(children)` is the query from the previous
section according to the concrete type of `children`, but without the
`select`, `limit`, `offset` or `order by` clauses. The overall structure of
this query then is

```sql
with matches as (
    select '{children.object}' as entity, c.vid, c.id,
           c.{sort_key}, p.id as parent_id
      from .. type_query(children) ..
     union all
       .. range over all child types ..
     order by {sort_key}
     limit {first} offset {skip})
select m.*, to_jsonb(c.*) as data
  from matches m, {children.table} c
 where c.vid = m.vid and m.entity = '{children.object}'
 union all
       .. range over all child tables ..
 order by {sort_key}
```

The list `all_parent_ids` must contain the ids of all the parents for which
we want to find children.

We have one `children` object for each concrete GraphQL type that we need
to query, where `children.table` is the name of the database table in which
these entities are stored, and `children.object` is the GraphQL typename
for these children.

The code uses an `EntityCollection::Window` containing multiple
`EntityWindow` instances to represent the most general form of querying for
the children of a set of parents, the query given above.

When there is only one window, we can simplify the above query. The
simplification basically inlines the `matches` CTE. That is important as
CTE's in Postgres before Postgres 12 are optimization fences, even when
they are only used once. We therefore reduce the two queries that Postgres
executes above to one for the fairly common case that the children are not
an interface. For each type of parent/child relationship, the resulting
query is essentially the same as the one given in the section
`Handling parent/child relationships`, except that the `select` clause is
changed to `select '{window.child_type}' as entity, to_jsonb(c.*) as data`:

```sql
select '..' as entity, to_jsonb(e.*) as data, p.id as parent_id
  from {expand_parents}
       cross join lateral
       (select *
          from children c
         where {linked_children}
           and .. other conditions on c ..
         order by c.{sort_key}
         limit {first} offset {skip}) c
 order by c.{sort_key}
```

Toplevel queries, i.e., queries where we have no parents, and therefore do
not restrict the children we return by parent ids are represented in the
code by an `EntityCollection::All`. If the GraphQL type of the children is
an interface with multiple implementers, we can simplify the query by
avoiding ranking and just using an ordinary `order by` clause:

```sql
with matches as (
  -- Get uniform info for all matching children
  select '{entity_type}' as entity, id, vid, {sort_key}
    from {entity_table} c
   where {query_filter}
   union all
     ... range over all entity types
   order by {sort_key} offset {query.skip} limit {query.first})
-- Get the full entity for each match
select m.entity, to_jsonb(c.*) as data, c.id, c.{sort_key}
  from matches m, {entity_table} c
 where c.vid = m.vid and m.entity = '{entity_type}'
 union all
       ... range over all entity types
 -- Make sure we return the children for each parent in the correct order
     order by c.{sort_key}, c.id
```

And finally, for the very common case of a toplevel GraphQL query for a
concrete type, not an interface, we can further simplify this, again by
essentially inlining the `matches` CTE to:

```sql
select '{entity_type}' as entity, to_jsonb(c.*) as data
  from {entity_table} c
 where query.filter()
 order by {query.order} offset {query.skip} limit {query.first}
```

## Boring list of possible GraphQL models

These are the eight ways in which a parent/child relationship can be
modeled. For brevity, the `id` attribute on each parent and child type has
been left out.

This list assumes that parent and child types are concrete types, i.e., that
any interfaces involved in this query have already been resolved into their
implementations and we are dealing with one pair of concrete parent/child
types.

```graphql
# Case 1
type Parent {
  children: [Child] @derived
}

type Child {
  parents: [Parent]
}

# Case 2
type Parent {
  child: Child @derived
}

type Child {
  parents: [Parent]
}

# Case 3
type Parent {
  children: [Child] @derived
}

type Child {
  parent: Parent
}

# Case 4
type Parent {
  child: Child @derived
}

type Child {
  parent: Parent
}

# Case 5
type Parent {
  children: [Child]
}

type Child {
  # doesn't matter
}

# Case 6
type Parent {
  children: [Child]
}

type Child {
  # doesn't matter
}

# Case 7
type Parent {
  child: Child
}

type Child {
  # doesn't matter
}

# Case 8
type Parent {
  child: Child
}

type Child {
  # doesn't matter
}
```

## Resources

* [PostgreSQL Manual](https://www.postgresql.org/docs/12/index.html)
* [Browsable SQL Grammar](https://jakewheat.github.io/sql-overview/sql-2016-foundation-grammar.html)
* [Wikipedia entry on ANSI SQL:2016](https://en.wikipedia.org/wiki/SQL:2016) The actual standard is not freely available
