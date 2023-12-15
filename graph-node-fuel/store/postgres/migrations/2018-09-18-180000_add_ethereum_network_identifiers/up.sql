/**************************************************************
* ADD etherum_networks COLUMNS
**************************************************************/

ALTER TABLE ethereum_networks
	ADD COLUMN net_version VARCHAR,
	ADD COLUMN genesis_block_hash VARCHAR,
	ADD CHECK ((net_version IS NULL) = (genesis_block_hash IS NULL));
