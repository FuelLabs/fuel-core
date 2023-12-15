alter table unused_deployments
      add column new_id int;

-- Recover the deployment_schemas.id from existing unused_deployments
-- entries
update unused_deployments
   set new_id = replace(namespace, 'sgd', '')::int;

alter table unused_deployments
      rename column id to deployment;

alter table unused_deployments
      rename column new_id to id;

alter table unused_deployments
      drop constraint unused_deployments_pkey;

alter table unused_deployments
      add primary key(id);
