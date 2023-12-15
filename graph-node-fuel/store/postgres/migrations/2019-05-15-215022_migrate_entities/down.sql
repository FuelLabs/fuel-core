delete from deployment_schemas
 where version='public';

drop function migrate_entities_tables(varchar,
                                      deployment_schema_version,
                                      varchar);
drop function migrate_entities_data(varchar,
                                    deployment_schema_version,
                                    varchar);

alter table deployment_schemas
  drop column version,
  drop column migrating,
  drop column state;

drop type deployment_schema_version;
drop type deployment_schema_state;
