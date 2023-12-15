-- List all metadata entities for a set of subgraph deployments.
-- The entities are returned as a table of (id, entity) pairs, in no particular
-- order. The list contains all the entities that are reachable through
-- associations from a SubgraphDeployment, and include the SubgraphDeployment
-- itself.
create or replace function
  list_deployment_entities(ids varchar[])
returns table(id varchar, entity varchar) as
$$
begin
  create temp table tmp_ids
      on commit drop as
  select * from unnest(ids) as id;
  create index tmp_ids_id on tmp_ids(id);

  -- Put the ids of all dynamic data sources for the
  -- subgraph deployments in ids into tmp_dds
  create temp table tmp_dds
      on commit drop as
  select dds.id
    from subgraphs.entities dds
   where dds.entity = 'DynamicEthereumContractDataSource'
     and dds.data->'deployment'->>'data' = any(ids);
   create index tmp_dds_id on tmp_dds(id);

   -- The metadata entities for a subgraph deployment consist of two
   -- distinct groups of entities:
   --   * static metadata, whose id starts with the id of the
   --     subgraph deployment. Subgraph deployment ids are 46 chars long
   --   * metadata for dynamic data sources, whose ids start with
   --     the id of the dynamic data source. Dynamic data source ids are
   --     40 chars long
   return query
   select e.id, e.entity
     from subgraphs.entities e, tmp_ids
    where substring(e.id, 1, 46) = tmp_ids.id
   union all
   select e.id, e.entity
     from subgraphs.entities e, tmp_dds
    where substring(e.id, 1, 40) = tmp_dds.id;
end;
$$ language plpgsql;

-- Remove the metadata for the deployments with the given ids. None of the
-- deployments must currently be assigned to a node for indexing.
-- Returns the number of entities removed.
-- Note that this function only removes the metadata from
-- subgraphs.entities, but not the actual data for the subgraph
create or replace function
  remove_deployment_metadata(ids varchar[])
returns int as
$$
declare
  rows_deleted int;
begin
  if exists (select 1
               from subgraphs.entities
              where entity = 'SubgraphDeploymentAssignment'
                and id = any(ids)) then
     raise 'Can not remove assigned subgraph. One of % is currently assigned',
           ids;
  end if;

  drop table if exists tmp_deployments;
  create temp table tmp_deployments
      on commit drop as
  select * from list_deployment_entities(ids);

  delete from subgraphs.entities as e
  using tmp_deployments as md
  where e.entity = md.entity and e.id = md.id;
  GET DIAGNOSTICS rows_deleted = ROW_COUNT;
  return rows_deleted;
end;
$$ language plpgsql;

-- Remove all subgrah deployments that are stored in public.entitites.
-- Both the subgraph data and its metadata will be deleted.
create or replace function
  remove_public_deployments()
returns int as
$$
declare
  subgraphs text[];
  rows_deleted int;
begin
  select array_agg(subgraph)
    into subgraphs
    from deployment_schemas
   where version = 'public';

  select remove_deployment_metadata(subgraphs)
    into rows_deleted;

  delete from deployment_schemas
   where subgraph = any(subgraphs);

  -- We would really like to remove unneeded events from event_meta_data; but
  -- doing this the obvious way with a 'delete' statement caused problems
  -- on a test copy of the production database (the pg process errors trying
  -- to write to the file containing the event_meta_data table, possibly
  -- because CloudSQL can not allocate storage fast enough)
  -- Since deleting this data is not crucial for the functioning of the
  -- system, just remember which event_id's correspond to entries
  -- in public.entity_history so we can clean them up if that ever becomes
  -- necessary
  create table unneeded_event_ids(
    event_id bigint primary key
  );
  insert into unneeded_event_ids
  select distinct event_id from public.entity_history;

  drop view  if exists public.entity_history_with_source;
  drop table if exists public.entities;
  drop table if exists public.entity_history;

  return rows_deleted;
end;
$$ language plpgsql;

select remove_public_deployments();
