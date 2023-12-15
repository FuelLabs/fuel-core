-- This is populated in the primary
create table active_copies(
   src          int not null references deployment_schemas(id),
   dst          int primary key
                    references deployment_schemas(id) on delete cascade,
   queued_at    timestamptz not null,
   cancelled_at timestamptz,
   unique(src, dst)
);

-- These two tables live in each shard; entries are in the same
-- shard as the subgraph_deployment we are copying to ('dst')
create table subgraphs.copy_state(
   src                 int not null,
   dst                 int primary key
                           references subgraphs.subgraph_deployment(id)
                                  on delete cascade,
   target_block_hash   bytea not null,
   target_block_number int not null,
   started_at          timestamptz not null default now(),
   finished_at         timestamptz,
   cancelled_at        timestamptz
);

create table subgraphs.copy_table_state(
   id            serial primary key,
   entity_type   text not null,
   dst           int not null
                     references subgraphs.copy_state(dst) on delete cascade,
   next_vid      int8 not null,
   target_vid    int8 not null,
   batch_size    int8 not null,
   started_at    timestamptz not null default now(),
   finished_at   timestamptz,
   duration_ms   int8 not null default 0,
   unique(dst, entity_type)
);
