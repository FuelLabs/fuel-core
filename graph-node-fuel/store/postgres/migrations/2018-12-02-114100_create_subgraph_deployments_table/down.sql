DROP TRIGGER after_deployment_insert ON subgraph_deployments;
DROP TRIGGER after_deployment_update ON subgraph_deployments;
DROP TRIGGER after_deployment_delete ON subgraph_deployments;

DROP FUNCTION deployment_insert();
DROP FUNCTION deployment_update();
DROP FUNCTION deployment_delete();

DROP TABLE subgraph_deployments;
