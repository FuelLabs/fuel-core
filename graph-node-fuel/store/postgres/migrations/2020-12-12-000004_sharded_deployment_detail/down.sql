-- The view as created in 2020-05-16-225611_add_deployment_detail_view
drop view subgraphs.subgraph_deployment_detail;
create view subgraphs.subgraph_deployment_detail as
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
