alter table deployment_schemas
  add column shard text;
update deployment_schemas
   set shard = 'primary';
alter table deployment_schemas
  alter column shard set not null;
