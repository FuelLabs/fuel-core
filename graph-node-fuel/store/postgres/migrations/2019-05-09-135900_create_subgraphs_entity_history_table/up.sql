-- Create an entity_history table for the subgraph of subgraphs.
create table subgraphs.entity_history (
  id          serial primary key,
  event_id    integer references event_meta_data(id)
                on update cascade
                on delete cascade,
  subgraph    varchar not null,
  entity      varchar not null,
  entity_id   varchar not null,
  data_before jsonb,
  reversion   bool not null default false,
  op_id       int2 not null
);
