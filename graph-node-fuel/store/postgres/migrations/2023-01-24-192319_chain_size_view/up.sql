
drop materialized view if exists info.chain_sizes;

create materialized view info.chain_sizes as
select *,
       pg_size_pretty(total_bytes) as total,
       pg_size_pretty(index_bytes) as index,
       pg_size_pretty(toast_bytes) as toast,
       pg_size_pretty(table_bytes) as table
  from (
    select *,
           total_bytes-index_bytes-coalesce(toast_bytes,0) AS table_bytes
      from (
        select nspname as table_schema, relname as table_name,
               'shared'::text as version,
               c.reltuples as row_estimate,
               pg_total_relation_size(c.oid) as total_bytes,
               pg_indexes_size(c.oid) as index_bytes,
               pg_total_relation_size(reltoastrelid) as toast_bytes
          from pg_class c
               join pg_namespace n on n.oid = c.relnamespace
          where relkind = 'r'
            and nspname like 'chain%'
  ) a
) a with no data;

drop view if exists info.all_sizes;

create view info.all_sizes as
select * from info.subgraph_sizes
union all
select * from info.chain_sizes
union all
select * from info.table_sizes;
