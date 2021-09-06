create schema if not exists test_namespace;

create table if not exists test_namespace.thing (
    id bigint primary key,
    account varchar(64) not null,
    object bytea not null
);

create table if not exists test_namespace.thing2 (
    id bigint primary key,
    account varchar(64) not null,
    hash varchar(64) not null,
    object bytea
);

insert into graph_registry.type_ids
    (id, schema_version, schema_name, table_name)
values
    (cast(x'B2BB8366E42C8896' as bigint), 'booooooooo', 'test_namespace', 'thing'),
    (cast(x'D5571E9C13557615' as bigint), 'booooooooo', 'test_namespace', 'thing2');

insert into graph_registry.columns
    (type_id, column_position, column_name, column_type)
values
    (cast(x'B2BB8366E42C8896' as bigint), 1, 'id', 0),
    (cast(x'B2BB8366E42C8896' as bigint), 2, 'account', 1),
    (cast(x'B2BB8366E42C8896' as bigint), 3, 'object', -1),
    (cast(x'D5571E9C13557615' as bigint), 1, 'id', 0),
    (cast(x'D5571E9C13557615' as bigint), 2, 'account', 1),
    (cast(x'D5571E9C13557615' as bigint), 3, 'hash', 2),
    (cast(x'D5571E9C13557615' as bigint), 4, 'object', -1)
