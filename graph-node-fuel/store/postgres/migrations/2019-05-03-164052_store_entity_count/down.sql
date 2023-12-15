-- Remove entityCount from SubgraphDeployment
update subgraphs.entities
   set data = data - 'entityCount'
 where entity='SubgraphDeployment';
