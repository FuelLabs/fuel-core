-- See migration 2020-12-12-000004_sharded_deployment_detail
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
