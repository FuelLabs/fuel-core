-- This view needs to handle 'normal' subgraphs and the fake subgraphs that
-- the network indexer creates. Those don't have datasources, and we can
-- therefore not determine the network through the data source.  Instead,
-- we rely on the fact that their name is 'network_ethereum_${NETWORK}_v0'
-- and use that as the network
--
-- This is a variation of the view created in
-- 2020-05-16-225611_add_deployment_detail_view but does not join with
-- ethereum_networks and subgraph_deployment_assignment since they live
-- in different shards from the deployment metadata
drop view subgraphs.subgraph_deployment_detail;
create view subgraphs.subgraph_deployment_detail as
select sd.*,
       ecds.network
  from subgraphs.subgraph_deployment sd
    inner join
       subgraphs.subgraph_manifest sm
         on (sd.manifest = sm.id)
    inner join
       subgraphs.ethereum_contract_data_source ecds
         on (ecds.id = sm.data_sources[1])
union all
select sd.*,
       split_part(sd.id, '_', 3) as network
  from subgraphs.subgraph_deployment sd
    inner join
       subgraphs.subgraph_manifest sm
         on (sd.manifest = sm.id and sm.data_sources[1] is null);
