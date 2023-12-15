alter table subgraphs.subgraph_deployment
  drop column ethereum_head_block_hash;
alter table subgraphs.subgraph_deployment
  drop column ethereum_head_block_number;
alter table subgraphs.subgraph_deployment
  drop column total_ethereum_blocks_count;

-- This view needs to handle 'normal' subgraphs and the fake subgraphs that
-- the network indexer creates. Those don't have datasources, and we can
-- therefore not determine the network through the data source.  Instead,
-- we rely on the fact that their name is 'network_ethereum_${NETWORK}_v0'
-- and use that as the network
create or replace view subgraphs.subgraph_deployment_detail as
select sd.*,
       decode(en.head_block_hash,'hex') as ethereum_head_block_hash,
       en.head_block_number as ethereum_head_block_number,
       ecds.network,
       sda.node_id
  from subgraphs.subgraph_deployment sd
    inner join
       subgraphs.subgraph_manifest sm
         on (sd.manifest = sm.id)
    inner join
       subgraphs.ethereum_contract_data_source ecds
         on (ecds.id = sm.data_sources[1])
    inner join
       ethereum_networks en
         on (en.name = ecds.network)
    left outer join
       subgraphs.subgraph_deployment_assignment sda
         on (sd.id = sda.id)
union all
select sd.*,
       decode(en.head_block_hash,'hex') as ethereum_head_block_hash,
       en.head_block_number as ethereum_head_block_number,
       split_part(sd.id, '_', 3) as network,
       sda.node_id
  from subgraphs.subgraph_deployment sd
    inner join
       subgraphs.subgraph_manifest sm
         on (sd.manifest = sm.id and sm.data_sources[1] is null)
    inner join
       ethereum_networks en
         on (en.name = split_part(sd.id, '_', 3))
    left outer join
       subgraphs.subgraph_deployment_assignment sda
         on (sd.id = sda.id);
