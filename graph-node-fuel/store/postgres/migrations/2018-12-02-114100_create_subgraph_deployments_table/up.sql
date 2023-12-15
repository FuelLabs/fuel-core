-- Maps names to immutable subgraph versions (IDs) and node IDs.
CREATE TABLE IF NOT EXISTS subgraph_deployments (
    deployment_name VARCHAR PRIMARY KEY,
    subgraph_id VARCHAR UNIQUE NOT NULL,
    node_id VARCHAR NOT NULL
);

CREATE OR REPLACE FUNCTION deployment_insert()
    RETURNS trigger AS
$$
BEGIN
    PERFORM pg_notify(CONCAT('subgraph_deployments_', NEW.node_id), json_build_object(
        'type', 'Add',
        'deployment_name', NEW.deployment_name,
        'subgraph_id', NEW.subgraph_id,
        'node_id', NEW.node_id
    )::text);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION deployment_update()
    RETURNS trigger AS
$$
BEGIN
    PERFORM pg_notify(CONCAT('subgraph_deployments_', OLD.node_id), json_build_object(
        'type', 'Remove',
        'deployment_name', OLD.deployment_name,
        'subgraph_id', OLD.subgraph_id,
        'node_id', OLD.node_id
    )::text);
    PERFORM pg_notify(CONCAT('subgraph_deployments_', NEW.node_id), json_build_object(
        'type', 'Add',
        'deployment_name', NEW.deployment_name,
        'subgraph_id', NEW.subgraph_id,
        'node_id', NEW.node_id
    )::text);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION deployment_delete()
    RETURNS trigger AS
$$
BEGIN
    PERFORM pg_notify(CONCAT('subgraph_deployments_', OLD.node_id), json_build_object(
        'type', 'Remove',
        'deployment_name', OLD.deployment_name,
        'subgraph_id', OLD.subgraph_id,
        'node_id', OLD.node_id
    )::text);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER after_deployment_insert
    AFTER INSERT ON subgraph_deployments
    FOR EACH ROW EXECUTE PROCEDURE deployment_insert();

CREATE TRIGGER after_deployment_update
    AFTER UPDATE ON subgraph_deployments
    FOR EACH ROW EXECUTE PROCEDURE deployment_update();

CREATE TRIGGER after_deployment_delete
    AFTER DELETE ON subgraph_deployments
    FOR EACH ROW EXECUTE PROCEDURE deployment_delete();
