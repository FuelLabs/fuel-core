alter type deployment_schema_state
  rename to deployment_schema_state_old;

create type deployment_schema_state
  as enum('ready','tables','init');

alter table deployment_schemas
  alter column state drop default;

alter table deployment_schemas
  alter column state type deployment_schema_state
  using state::text::deployment_schema_state;

alter table deployment_schemas
  alter column state set default 'init';

drop type deployment_schema_state_old;
