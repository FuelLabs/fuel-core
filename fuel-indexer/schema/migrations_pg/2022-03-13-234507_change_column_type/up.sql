-- Your SQL goes here
ALTER TABLE graph_registry.columns
ALTER COLUMN column_type TYPE varchar;

ALTER TABLE graph_registry.graph_root
ALTER COLUMN id TYPE integer;

ALTER TABLE graph_registry.root_columns
ALTER COLUMN root_id TYPE integer;

DROP TYPE ColumnTypeName;
