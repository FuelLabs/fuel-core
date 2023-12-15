create function store_entity_count()
returns void
language plpgsql
as $function$
declare
  deployment   record;
  entity_count int8;
begin
  -- Initialize the entityCount for all subgraphs
  update subgraphs.entities e
  set data = data || '{"entityCount": {"data": "0", "type": "BigInt"}}'
  where e.entity='SubgraphDeployment';

  -- Handle public.entities
  with entity_counts as (
    select subgraph, count(*) as count
      from public.entities
    group by subgraph)
  update subgraphs.entities e
  set data = data ||
             format('{"entityCount": {"data": "%s", "type": "BigInt"}}',
                    (select count from entity_counts c
                                 where c.subgraph=e.id))::jsonb
  where e.entity='SubgraphDeployment'
    and e.id in (select subgraph from entity_counts);

  -- Handle split entities tables
  for deployment in
    -- Join with pg_namespace to make sure we do not, under any
    -- circumstances, try to access a schema that was dropped for
    -- whatever reason
    select s.* from deployment_schemas s, pg_namespace n
            where subgraph != 'subgraphs'
              and s.name = n.nspname
  loop
    execute format('select count(*) from %I.entities', deployment.name)
       into entity_count;
    update subgraphs.entities e
    set data = data ||
             format('{"entityCount": {"data": "%s", "type": "BigInt"}}',
                    entity_count)::jsonb
    where e.entity='SubgraphDeployment'
      and e.id = deployment.subgraph;
  end loop;
end;
$function$;

select store_entity_count();

drop function store_entity_count();
