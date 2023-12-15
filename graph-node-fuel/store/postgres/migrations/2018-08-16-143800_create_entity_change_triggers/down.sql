/**************************************************************
 * DROP TRIGGERS
 **************************************************************/

DROP TRIGGER IF EXISTS entity_added ON entities;
DROP TRIGGER IF EXISTS entity_updated ON entities;
DROP TRIGGER IF EXISTS entity_removed ON entities;

/**************************************************************
 * DROP FUNCTIONS
 **************************************************************/

DROP FUNCTION IF EXISTS notify_entity_added;
DROP FUNCTION IF EXISTS notify_entity_updated;
DROP FUNCTION IF EXISTS notify_entity_removed;
