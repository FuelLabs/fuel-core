-- We can't add a value to an existing enum inside a transaction. Because
-- of that, we rename the old one and create a brand new type.

drop function
  migrate_entities_tables(varchar, deployment_schema_version, varchar);
drop function
  migrate_entities_data(varchar, deployment_schema_version, varchar);

alter type deployment_schema_version
  rename to deployment_schema_version_old;

create type deployment_schema_version
  as enum('split', 'relational');

alter table deployment_schemas
  alter column version type deployment_schema_version
  using version::text::deployment_schema_version;

drop type deployment_schema_version_old;
