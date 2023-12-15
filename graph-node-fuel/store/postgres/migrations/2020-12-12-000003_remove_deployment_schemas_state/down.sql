create type deployment_schema_state
  as enum('ready','tables','init');

alter table deployment_schemas
  add column state deployment_schema_state not null default 'init';
