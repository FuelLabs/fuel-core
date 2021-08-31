create schema graph_registry;

create table graph_registry.type_ids (
    id bigint primary key,
    schema_name varchar(32) not null,
    table_name varchar(32) not null
);

create table graph_registry.columns (
    id serial primary key,
    type_id bigint not null,
    column_position integer not null,
    column_name varchar(32) not null,
    column_type integer not null,
    constraint fk_table_name
        foreign key(type_id)
            references graph_registry.type_ids(id)
);


