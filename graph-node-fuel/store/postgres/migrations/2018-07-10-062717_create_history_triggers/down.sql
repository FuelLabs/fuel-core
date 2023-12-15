/**************************************************************
* DROP TRIGGERS
**************************************************************/
DROP TRIGGER before_transaction_trigger ON entities;
DROP TRIGGER after_insert_trigger ON entities;
DROP TRIGGER after_update_trigger ON entities;
DROP TRIGGER after_delete_trigger ON entities;

/**************************************************************
* DROP FUNCTIONS
**************************************************************/
DROP FUNCTION log_transaction();
DROP FUNCTION log_insert();
DROP FUNCTION log_update();
DROP FUNCTION log_delete();
