create table eth_call_cache (
  id bytea primary key,
  return_value bytea not null,
  contract_address bytea not null,
  block_number integer not null
);

create table eth_call_meta (
   contract_address bytea primary key,
   accessed_at date not null
);
