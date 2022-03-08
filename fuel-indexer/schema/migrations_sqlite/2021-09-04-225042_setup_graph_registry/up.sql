-- Your SQL goes here
create table type_ids (
    id bigint primary key not null,
    schema_version varchar(512) not null,
    schema_name varchar(32) not null,
    graphql_name varchar(32) not null,
    table_name varchar(32) not null
);

create table columns (
    id integer primary key not null,
    type_id bigint not null,
    column_position integer not null,
    column_name varchar(32) not null,
    column_type varchar not null,
    nullable boolean not null,
    graphql_type varchar not null,
    constraint fk_table_name
        foreign key(type_id)
            references type_ids(id)
);

CREATE TABLE IF NOT EXISTS graph_root (
    id integer primary key autoincrement not null,
    version varchar not null,
    schema_name varchar not null,
    query varchar not null,
    schema varchar not null,
    UNIQUE(version, schema_name)
);

CREATE TABLE IF NOT EXISTS root_columns (
    id integer primary key not null,
    root_id integer not null,
    column_name varchar(32) not null,
    graphql_type varchar(32) not null,
    CONSTRAINT fk_root_id
        FOREIGN KEY(root_id)
            REFERENCES graph_root(id)
);
