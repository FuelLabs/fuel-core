-- Your SQL goes here
CREATE TABLE IF NOT EXISTS graph_registry.graph_root (
    id bigserial primary key,
    version varchar not null,
    schema_name varchar not null,
    query varchar not null,
    schema varchar not null,
    UNIQUE(version, schema_name)
);

CREATE TABLE IF NOT EXISTS graph_registry.root_columns (
    id serial primary key,
    root_id bigserial not null,
    column_name varchar(32) not null,
    graphql_type varchar(32) not null,
    CONSTRAINT fk_root_id
        FOREIGN KEY(root_id)
            REFERENCES graph_registry.graph_root(id)
);

ALTER TABLE graph_registry.columns
ADD COLUMN graphql_type varchar not null;

ALTER TYPE ColumnTypeName ADD VALUE 'Int4';
ALTER TYPE ColumnTypeName ADD VALUE 'Int8';
ALTER TYPE ColumnTypeName ADD VALUE 'UInt4';
ALTER TYPE ColumnTypeName ADD VALUE 'UInt8';
ALTER TYPE ColumnTypeName ADD VALUE 'Timestamp';
