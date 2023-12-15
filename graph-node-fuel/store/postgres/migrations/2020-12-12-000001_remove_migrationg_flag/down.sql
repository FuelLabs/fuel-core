alter table deployment_schemas
  add column migrating bool not null default false;
