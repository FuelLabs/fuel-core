alter table chains
  add column namespace text not null
             default 'chain' || currval('chains_id_seq');
-- all chain data so far lives in shared tables in 'public'
update chains set namespace = 'public';

alter table ethereum_networks
  add column namespace text;
update ethereum_networks set namespace = 'public';
alter table ethereum_networks
  alter column namespace set not null;
