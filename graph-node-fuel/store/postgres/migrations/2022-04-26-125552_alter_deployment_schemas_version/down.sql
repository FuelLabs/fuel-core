-- This file should undo anything in `up.sql`

-- This is dropped and left for the fdw logic which runs after migrations to recreate.
drop schema if exists primary_public cascade;

drop view if exists info.subgraph_info;
drop view if exists info.all_sizes;
drop materialized view if exists info.subgraph_sizes;

create type deployment_schema_version as enum('split', 'relational');
alter table if exists deployment_schemas alter version type deployment_schema_version using 'relational';

create view info.subgraph_info as
SELECT ds.id AS schema_id,
    ds.name AS schema_name,
    ds.subgraph,
    ds.version,
    s.name,
        CASE
            WHEN s.pending_version = v.id THEN 'pending'::text
            WHEN s.current_version = v.id THEN 'current'::text
            ELSE 'unused'::text
        END AS status,
    d.failed,
    d.synced
   FROM deployment_schemas ds,
    subgraphs.subgraph_deployment d,
    subgraphs.subgraph_version v,
    subgraphs.subgraph s
  WHERE d.deployment = ds.subgraph::text AND v.deployment = d.deployment AND v.subgraph = s.id;

create materialized view info.subgraph_sizes as
select *,
       pg_size_pretty(total_bytes) as total,
       pg_size_pretty(index_bytes) as index,
       pg_size_pretty(toast_bytes) as toast,
       pg_size_pretty(table_bytes) as table
  from (
    select *,
           total_bytes-index_bytes-coalesce(toast_bytes,0) AS table_bytes
      from (
        select nspname as name,
               ds.subgraph as subgraph,
               ds.version::text as version,
               sum(c.reltuples) as row_estimate,
               sum(pg_total_relation_size(c.oid)) as total_bytes,
               sum(pg_indexes_size(c.oid)) as index_bytes,
               sum(pg_total_relation_size(reltoastrelid)) as toast_bytes
          from pg_class c
               join pg_namespace n on n.oid = c.relnamespace
               join deployment_schemas ds on ds."name" = n.nspname
          where relkind = 'r'
            and nspname like 'sgd%'
          group by nspname, subgraph, version
  ) a
) a with no data;

create view info.all_sizes as
select * from info.subgraph_sizes
union all
select * from info.table_sizes;
