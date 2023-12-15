create table unused_deployments (
	id           text primary key,
	unused_at    timestamptz not null default now(),
	removed_at   timestamptz,

    subgraphs    text[],
	namespace    text not null,
	shard        text not null,

	entity_count int4 not null default 0,
	latest_ethereum_block_hash bytea,
	latest_ethereum_block_number int4,
	failed       bool not null default false,
	synced       bool not null default false
);
