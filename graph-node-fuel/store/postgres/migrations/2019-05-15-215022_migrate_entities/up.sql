-- The different storage schemes/versions we support for subgraphs.
-- Over time, we will add more as our storage scheme for individual
-- subgraphs evolves
create type deployment_schema_version as enum ('public', 'split');
-- How far a currently running migration has gotten. See
-- store/postgres/src/entities.rs for more details
create type deployment_schema_state as enum ('ready', 'tables');

-- Modify deployment_schemas to track the progress of any needed migrations
alter table deployment_schemas
  add column version   deployment_schema_version,
  add column migrating bool not null default false,
  add column state     deployment_schema_state not null default 'ready';

-- Existing subgraphs (subgraph of subgraphs and subgraphs that have been
-- deployed since commit d2248402) are stored in their own split-out
-- database schema
update deployment_schemas
   set version='split';

alter table deployment_schemas
  alter column version set not null;

-- Legacy subgraphs (those deployed before commit d2248402) are stored in
-- the big public.entities table. With the migration machinery we are
-- putting in place, we need an entry in deployment_schemas for every
-- deployed subgraph
insert into deployment_schemas(subgraph, version)
select id, 'public'::deployment_schema_version
  from subgraphs.entities e
 where entity='SubgraphDeployment'
   and not exists (select 1 from deployment_schemas d
                           where d.subgraph=e.id);

-- Migrate the entities for one subgraph into their own schema. The
-- deployment_schemas table must already contain an entry for that
-- subgraph
create or replace function migrate_entities_tables(
  schema_name varchar,
  schema_version deployment_schema_version,
  subgraph_id varchar
) returns void
language plpgsql
as $function$
declare
  index_row  record;
begin
  if schema_version <> 'public' then
    raise 'Schema for %(%) has version %, but should have version ''public''',
          schema_name, subgraph_id, schema_version;
  end if;

  execute format('create schema %I', schema_name);

  -- Dirty trick: modify the search path; unqualified table references
  -- will be looked up in schema_name first
  perform set_config('search_path', format('%I, public', schema_name), true);

  -- Create the entities table and all its trimmings
  create table entities(
      entity       varchar not null,
      id           varchar not null,
      data         jsonb,
      event_source varchar not null,
      primary key(entity, id)
  );

  create table entity_history (
      id           serial primary key,
      event_id     integer references event_meta_data(id)
                             on update cascade on delete cascade,
      entity       varchar not null,
      entity_id    varchar not null,
      data_before  jsonb,
      reversion    bool not null default false,
      op_id        int2 NOT NULL
  );

  create index entity_history_event_id_btree_idx
      on entity_history(event_id);

  -- Set the search path back to the default so that we do not
  -- influence other database work
  perform set_config('search_path', '"$user", public', true);
end;
$function$;

create or replace function migrate_entities_data(
  schema_name varchar,
  schema_version deployment_schema_version,
  subgraph_id varchar
) returns integer
language plpgsql
as $function$
declare
  entities_count integer;
  index_row  record;
  has_rows bool;
begin
  if schema_version <> 'public' then
    raise 'Schema for %(%) has version %, but should have version ''public''',
          subgraph_id, schema_name, schema_version;
  end if;

  execute format('select exists (select 1 from %I.entities)', schema_name)
     into has_rows;
  if has_rows then
    raise 'Expected table %.entities to be empty', schema_name;
  end if;

  execute format('select exists (select 1 from %I.entity_history)', schema_name)
     into has_rows;
  if has_rows then
    raise 'Expected table %.entity_history to be empty', schema_name;
  end if;

  -- It is possible that a previous attempt to do the migration copied a
  -- lot of data and then was abandoned (for example, because the
  -- graph-node instance running it crashed) That leaves the entities and
  -- entity_history tables empty, but all the data that was copied before
  -- still exists as dead tuples. Truncate the tables to get rid of those
  -- dead tuples
  execute format('truncate %I.entities', schema_name);
  execute format('truncate %I.entity_history', schema_name);

  -- Migrate data out of the public schema
  execute format('
      insert into %I.entities
      select entity, id, data, event_source
        from public.entities p
        where p.subgraph=$1', schema_name)
  using subgraph_id;
  GET DIAGNOSTICS entities_count = ROW_COUNT;

  delete from public.entities
   where subgraph=subgraph_id;

  execute format('
      insert into
        %I.entity_history(event_id, entity, entity_id,
                          data_before, reversion, op_id)
      select event_id, entity, entity_id,
             data_before, reversion, op_id
        from public.entity_history
       where subgraph=$1', schema_name)
  using subgraph_id;

  delete from public.entity_history
   where subgraph=subgraph_id;

  -- Create change triggers after migrating the data
  -- Otherwise we wind up with duplicate history

  -- Need to set the search_path again so the trigges go on the right table
  perform set_config('search_path', format('%I, public', schema_name), true);
  create trigger entity_change_insert_trigger
      after insert on entities
      for each row
      execute procedure subgraph_log_entity_event();

  create trigger entity_change_update_trigger
      after update on entities
      for each row
      when (old.data != new.data)
      execute procedure subgraph_log_entity_event();

  create trigger entity_change_delete_trigger
      after delete on entities
      for each row
      execute procedure subgraph_log_entity_event();

  perform set_config('search_path', '"$user", public', true);

  -- We should drop attribute indexes for subgraph_id from public.entities
  -- at this point, but that requires an access exclusive lock on that
  -- table, which can lead to concurrently running migrations to deadlock.
  -- We therefore leave the indexes in place, and they will need to be
  -- cleaned up separately.

  -- Update statistics for our new tables
  execute format('analyze %I.entities', schema_name);
  execute format('analyze %I.entity_history', schema_name);

  return entities_count;
end;
$function$;
