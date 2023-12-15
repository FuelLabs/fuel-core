insert into public.entities(subgraph, entity, id, data, event_source)
select 'subgraphs', entity, id, data, event_source
from subgraphs.entities;


create or replace function drop_schemas() returns void
language plpgsql
as $function$
declare
  schema_name varchar;
begin
  for schema_name in
  select name from deployment_schemas
  loop
    execute 'drop schema ' || schema_name || ' cascade';
  end loop;
end
$function$;

select drop_schemas();

drop table deployment_schemas;

drop function drop_schemas();

drop function if exists subgraph_log_entity_event();
drop function if exists subgraph_revert_block(varchar, varchar);
