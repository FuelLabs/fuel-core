/**************************************************************
* REMOVE etherum_networks COLUMNS
**************************************************************/

ALTER TABLE ethereum_networks
	DROP COLUMN net_version,
	DROP COLUMN genesis_block_hash;
