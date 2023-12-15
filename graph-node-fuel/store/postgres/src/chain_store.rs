use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, PooledConnection};
use diesel::sql_types::Text;
use diesel::{insert_into, update};
use graph::env::ENV_VARS;
use graph::parking_lot::RwLock;
use graph::prelude::MetricsRegistry;
use graph::prometheus::{CounterVec, GaugeVec};

use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    iter::FromIterator,
    sync::Arc,
};

use graph::blockchain::{Block, BlockHash, ChainIdentifier};
use graph::cheap_clone::CheapClone;
use graph::prelude::web3::types::H256;
use graph::prelude::{
    async_trait, ethabi, serde_json as json, transaction_receipt::LightTransactionReceipt,
    BlockNumber, BlockPtr, CachedEthereumCall, CancelableError, ChainStore as ChainStoreTrait,
    Error, EthereumCallCache, StoreError,
};
use graph::{constraint_violation, ensure};

use self::recent_blocks_cache::RecentBlocksCache;
use crate::{
    block_store::ChainStatus, chain_head_listener::ChainHeadUpdateSender,
    connection_pool::ConnectionPool,
};

/// Our own internal notion of a block
#[derive(Clone, Debug)]
struct JsonBlock {
    ptr: BlockPtr,
    parent_hash: BlockHash,
    data: Option<json::Value>,
}

impl JsonBlock {
    fn new(ptr: BlockPtr, parent_hash: BlockHash, data: Option<json::Value>) -> Self {
        JsonBlock {
            ptr,
            parent_hash,
            data,
        }
    }
}

/// Tables in the 'public' database schema that store chain-specific data
mod public {
    table! {
        ethereum_networks (name) {
            name -> Varchar,
            namespace -> Varchar,
            head_block_hash -> Nullable<Varchar>,
            head_block_number -> Nullable<BigInt>,
            net_version -> Varchar,
            genesis_block_hash -> Varchar,
            head_block_cursor -> Nullable<Varchar>,
        }
    }
}

pub use data::Storage;

/// Encapuslate access to the blocks table for a chain.
mod data {
    use diesel::sql_types::{Array, Binary};
    use diesel::{connection::SimpleConnection, insert_into};
    use diesel::{delete, prelude::*, sql_query};
    use diesel::{dsl::sql, pg::PgConnection};
    use diesel::{
        pg::Pg,
        serialize::Output,
        sql_types::Text,
        types::{FromSql, ToSql},
    };
    use diesel::{
        sql_types::{BigInt, Bytea, Integer, Jsonb},
        update,
    };
    use graph::blockchain::{Block, BlockHash};
    use graph::constraint_violation;
    use graph::prelude::ethabi::ethereum_types::H160;
    use graph::prelude::transaction_receipt::LightTransactionReceipt;
    use graph::prelude::web3::types::H256;
    use graph::prelude::{
        serde_json as json, BlockNumber, BlockPtr, CachedEthereumCall, Error, StoreError,
    };
    use std::fmt;
    use std::iter::FromIterator;
    use std::str::FromStr;
    use std::{convert::TryFrom, io::Write};

    use crate::transaction_receipt::RawTransactionReceipt;

    use super::JsonBlock;

    pub(crate) const ETHEREUM_BLOCKS_TABLE_NAME: &str = "public.ethereum_blocks";

    pub(crate) const ETHEREUM_CALL_CACHE_TABLE_NAME: &str = "public.eth_call_cache";

    mod public {
        pub(super) use super::super::public::ethereum_networks;

        table! {
            ethereum_blocks (hash) {
                hash -> Varchar,
                number -> BigInt,
                parent_hash -> Nullable<Varchar>,
                network_name -> Varchar, // REFERENCES ethereum_networks (name),
                data -> Jsonb,
            }
        }

        allow_tables_to_appear_in_same_query!(ethereum_networks, ethereum_blocks);

        table! {
            /// `id` is the hash of contract address + encoded function call + block number.
            eth_call_cache (id) {
                id -> Bytea,
                return_value -> Bytea,
                contract_address -> Bytea,
                block_number -> Integer,
            }
        }

        table! {
            /// When was a cached call on a contract last used? This is useful to clean old data.
            eth_call_meta (contract_address) {
                contract_address -> Bytea,
                accessed_at -> Date,
            }
        }

        joinable!(eth_call_cache -> eth_call_meta (contract_address));
        allow_tables_to_appear_in_same_query!(eth_call_cache, eth_call_meta);
    }

    // Helper for literal SQL queries that look up a block hash
    #[derive(QueryableByName)]
    struct BlockHashText {
        #[sql_type = "Text"]
        hash: String,
    }

    #[derive(QueryableByName)]
    struct BlockHashBytea {
        #[sql_type = "Bytea"]
        hash: Vec<u8>,
    }

    // Like H256::from_slice, but returns an error instead of panicking
    // when `bytes` does not have the right length
    fn h256_from_bytes(bytes: &[u8]) -> Result<H256, StoreError> {
        if bytes.len() == H256::len_bytes() {
            Ok(H256::from_slice(bytes))
        } else {
            Err(constraint_violation!(
                "invalid H256 value `{}` has {} bytes instead of {}",
                graph::prelude::hex::encode(bytes),
                bytes.len(),
                H256::len_bytes()
            ))
        }
    }

    type DynTable = diesel_dynamic_schema::Table<String>;
    type DynColumn<ST> = diesel_dynamic_schema::Column<DynTable, &'static str, ST>;

    /// The table that holds blocks when we store a chain in its own
    /// dedicated database schema
    #[derive(Clone, Debug)]
    struct BlocksTable {
        /// The fully qualified name of the blocks table, including the
        /// schema
        qname: String,
        table: DynTable,
    }

    impl BlocksTable {
        const TABLE_NAME: &'static str = "blocks";

        fn new(namespace: &str) -> Self {
            BlocksTable {
                qname: format!("{}.{}", namespace, Self::TABLE_NAME),
                table: diesel_dynamic_schema::schema(namespace.to_string())
                    .table(Self::TABLE_NAME.to_string()),
            }
        }

        fn table(&self) -> DynTable {
            self.table.clone()
        }

        fn hash(&self) -> DynColumn<Bytea> {
            self.table.column::<Bytea, _>("hash")
        }

        fn number(&self) -> DynColumn<BigInt> {
            self.table.column::<BigInt, _>("number")
        }

        fn parent_hash(&self) -> DynColumn<Bytea> {
            self.table.column::<Bytea, _>("parent_hash")
        }

        fn data(&self) -> DynColumn<Jsonb> {
            self.table.column::<Jsonb, _>("data")
        }
    }

    #[derive(Clone, Debug)]
    struct CallMetaTable {
        qname: String,
        table: DynTable,
    }

    impl CallMetaTable {
        const TABLE_NAME: &'static str = "call_meta";
        const ACCESSED_AT: &'static str = "accessed_at";

        fn new(namespace: &str) -> Self {
            CallMetaTable {
                qname: format!("{}.{}", namespace, Self::TABLE_NAME),
                table: diesel_dynamic_schema::schema(namespace.to_string())
                    .table(Self::TABLE_NAME.to_string()),
            }
        }

        fn table(&self) -> DynTable {
            self.table.clone()
        }

        fn contract_address(&self) -> DynColumn<Bytea> {
            self.table.column::<Bytea, _>("contract_address")
        }
    }

    #[derive(Clone, Debug)]
    struct CallCacheTable {
        qname: String,
        table: DynTable,
    }

    impl CallCacheTable {
        const TABLE_NAME: &'static str = "call_cache";

        fn new(namespace: &str) -> Self {
            CallCacheTable {
                qname: format!("{}.{}", namespace, Self::TABLE_NAME),
                table: diesel_dynamic_schema::schema(namespace.to_string())
                    .table(Self::TABLE_NAME.to_string()),
            }
        }

        fn table(&self) -> DynTable {
            self.table.clone()
        }

        fn id(&self) -> DynColumn<Bytea> {
            self.table.column::<Bytea, _>("id")
        }

        fn block_number(&self) -> DynColumn<BigInt> {
            self.table.column::<BigInt, _>("block_number")
        }

        fn return_value(&self) -> DynColumn<Bytea> {
            self.table.column::<Bytea, _>("return_value")
        }

        fn contract_address(&self) -> DynColumn<Bytea> {
            self.table.column::<Bytea, _>("contract_address")
        }
    }

    #[derive(Clone, Debug)]
    pub struct Schema {
        name: String,
        blocks: BlocksTable,
        call_meta: CallMetaTable,
        call_cache: CallCacheTable,
    }

    impl Schema {
        fn new(name: String) -> Self {
            let blocks = BlocksTable::new(&name);
            let call_meta = CallMetaTable::new(&name);
            let call_cache = CallCacheTable::new(&name);
            Self {
                name,
                blocks,
                call_meta,
                call_cache,
            }
        }
    }

    #[derive(Clone, Debug, AsExpression, FromSqlRow)]
    #[sql_type = "diesel::sql_types::Text"]
    /// Storage for a chain. The underlying namespace (database schema) is either
    /// `public` or of the form `chain[0-9]+`.
    pub enum Storage {
        /// Chain data is stored in shared tables
        Shared,
        /// The chain has its own namespace in the database with dedicated
        /// tables
        Private(Schema),
    }

    impl fmt::Display for Storage {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            match self {
                Self::Shared => Self::PUBLIC.fmt(f),
                Self::Private(Schema { name, .. }) => name.fmt(f),
            }
        }
    }

    impl FromSql<Text, Pg> for Storage {
        fn from_sql(bytes: Option<&[u8]>) -> diesel::deserialize::Result<Self> {
            let s = <String as FromSql<Text, Pg>>::from_sql(bytes)?;
            Self::new(s).map_err(Into::into)
        }
    }

    impl ToSql<Text, Pg> for Storage {
        fn to_sql<W: Write>(&self, out: &mut Output<W, Pg>) -> diesel::serialize::Result {
            <String as ToSql<Text, Pg>>::to_sql(&self.to_string(), out)
        }
    }

    impl Storage {
        const PREFIX: &'static str = "chain";
        const PUBLIC: &'static str = "public";

        fn new(s: String) -> Result<Self, String> {
            if s.as_str() == Self::PUBLIC {
                return Ok(Self::Shared);
            }

            if !s.starts_with(Self::PREFIX) || s.len() <= Self::PREFIX.len() {
                return Err(s);
            }
            for c in s.chars().skip(Self::PREFIX.len()) {
                if !c.is_numeric() {
                    return Err(s);
                }
            }

            Ok(Self::Private(Schema::new(s)))
        }

        /// Create dedicated database tables for this chain if it uses
        /// `Storage::Private`. If it uses `Storage::Shared`, do nothing since
        /// a regular migration will already have created the `ethereum_blocks`
        /// table
        pub(super) fn create(&self, conn: &PgConnection) -> Result<(), Error> {
            fn make_ddl(nsp: &str) -> String {
                format!(
                    "
                create schema {nsp};
                create table {nsp}.blocks (
                  hash         bytea  not null primary key,
                  number       int8  not null,
                  parent_hash  bytea  not null,
                  data         jsonb not null
                );
                create index blocks_number ON {nsp}.blocks using btree(number);

                create table {nsp}.call_cache (
	              id               bytea not null primary key,
	              return_value     bytea not null,
	              contract_address bytea not null,
	              block_number     int4 not null
                );
                create index call_cache_block_number_idx ON {nsp}.call_cache(block_number);

                create table {nsp}.call_meta (
                    contract_address bytea not null primary key,
                    accessed_at      date  not null
                );
            ",
                    nsp = nsp
                )
            }

            match self {
                Storage::Shared => Ok(()),
                Storage::Private(Schema { name, .. }) => {
                    conn.batch_execute(&make_ddl(name))?;
                    Ok(())
                }
            }
        }

        /// Returns a fully qualified table name to the blocks table
        #[inline]
        fn blocks_table(&self) -> &str {
            match self {
                Storage::Shared => ETHEREUM_BLOCKS_TABLE_NAME,
                Storage::Private(Schema { blocks, .. }) => &blocks.qname,
            }
        }

        pub(super) fn drop_storage(
            &self,
            conn: &PgConnection,
            name: &str,
        ) -> Result<(), StoreError> {
            match &self {
                Storage::Shared => {
                    use public::ethereum_blocks as b;
                    delete(b::table.filter(b::network_name.eq(name))).execute(conn)?;
                    Ok(())
                }
                Storage::Private(Schema { name, .. }) => {
                    conn.batch_execute(&format!("drop schema {} cascade", name))?;
                    Ok(())
                }
            }
        }

        pub(super) fn truncate_block_cache(&self, conn: &PgConnection) -> Result<(), StoreError> {
            let table_name = match &self {
                Storage::Shared => ETHEREUM_BLOCKS_TABLE_NAME,
                Storage::Private(Schema { blocks, .. }) => &blocks.qname,
            };
            conn.batch_execute(&format!("truncate table {} restart identity", table_name))?;
            Ok(())
        }

        fn truncate_call_cache(&self, conn: &PgConnection) -> Result<(), StoreError> {
            let table_name = match &self {
                Storage::Shared => ETHEREUM_CALL_CACHE_TABLE_NAME,
                Storage::Private(Schema { call_cache, .. }) => &call_cache.qname,
            };
            conn.batch_execute(&format!("truncate table {} restart identity", table_name))?;
            Ok(())
        }

        pub(super) fn cleanup_shallow_blocks(
            &self,
            conn: &PgConnection,
            lowest_block: i32,
        ) -> Result<(), StoreError> {
            let table_name = match &self {
                Storage::Shared => ETHEREUM_BLOCKS_TABLE_NAME,
                Storage::Private(Schema { blocks, .. }) => &blocks.qname,
            };
            conn.batch_execute(&format!(
                "delete from {} WHERE number >= {} AND data->'block'->'data' = 'null'::jsonb;",
                table_name, lowest_block,
            ))?;
            Ok(())
        }

        pub(super) fn remove_cursor(
            &self,
            conn: &PgConnection,
            chain: &str,
        ) -> Result<Option<BlockNumber>, StoreError> {
            use diesel::dsl::not;
            use public::ethereum_networks::dsl::*;

            match update(
                ethereum_networks
                    .filter(name.eq(chain))
                    .filter(not(head_block_cursor.is_null())),
            )
            .set(head_block_cursor.eq(None as Option<String>))
            .returning(head_block_number)
            .get_result::<Option<i64>>(conn)
            .optional()
            {
                Ok(res) => match res {
                    Some(opt_num) => match opt_num {
                        Some(num) => Ok(Some(num as i32)),
                        None => Ok(None),
                    },
                    None => Ok(None),
                },
                Err(e) => Err(e),
            }
            .map_err(Into::into)
        }

        /// Insert a block. If the table already contains a block with the
        /// same hash, then overwrite that block since it may be adding
        /// transaction receipts. If `overwrite` is `true`, overwrite a
        /// possibly existing entry. If it is `false`, keep the old entry.
        pub(super) fn upsert_block(
            &self,
            conn: &PgConnection,
            chain: &str,
            block: &dyn Block,
            overwrite: bool,
        ) -> Result<(), StoreError> {
            // Hash indicating 'no parent'. It seems to be customary at
            // least on EVM-compatible chains to fill the parent hash of the
            // genesis block with this value
            const NO_PARENT: &str =
                "0000000000000000000000000000000000000000000000000000000000000000";

            let number = block.number() as i64;
            let data = block.data().expect("Failed to serialize block");
            let hash = block.hash();
            let parent_hash = block.parent_hash().unwrap_or_else(|| {
                BlockHash::try_from(NO_PARENT).expect("NO_PARENT is a valid hash")
            });

            match self {
                Storage::Shared => {
                    use public::ethereum_blocks as b;

                    let values = (
                        b::hash.eq(hash.hash_hex()),
                        b::number.eq(number),
                        b::parent_hash.eq(parent_hash.hash_hex()),
                        b::network_name.eq(chain),
                        b::data.eq(data),
                    );

                    if overwrite {
                        insert_into(b::table)
                            .values(values.clone())
                            .on_conflict(b::hash)
                            .do_update()
                            .set(values)
                            .execute(conn)?;
                    } else {
                        insert_into(b::table)
                            .values(values.clone())
                            .on_conflict(b::hash)
                            .do_nothing()
                            .execute(conn)?;
                    }
                }
                Storage::Private(Schema { blocks, .. }) => {
                    let query = if overwrite {
                        format!(
                            "insert into {}(hash, number, parent_hash, data) \
                             values ($1, $2, $3, $4) \
                                 on conflict(hash) \
                                 do update set number = $2, parent_hash = $3, data = $4",
                            blocks.qname,
                        )
                    } else {
                        format!(
                            "insert into {}(hash, number, parent_hash, data) \
                             values ($1, $2, $3, $4) \
                                 on conflict(hash) do nothing",
                            blocks.qname
                        )
                    };
                    sql_query(query)
                        .bind::<Bytea, _>(hash.as_slice())
                        .bind::<BigInt, _>(number)
                        .bind::<Bytea, _>(parent_hash.as_slice())
                        .bind::<Jsonb, _>(data)
                        .execute(conn)?;
                }
            };
            Ok(())
        }

        pub(super) fn blocks(
            &self,
            conn: &PgConnection,
            chain: &str,
            hashes: &[BlockHash],
        ) -> Result<Vec<JsonBlock>, Error> {
            use diesel::dsl::any;

            // We need to deal with chain stores where some entries have a
            // toplevel 'block' field and others directly contain what would
            // be in the 'block' field. Make sure we return the contents of
            // the 'block' field if it exists, otherwise assume the whole
            // Json object is what should be in 'block'
            //
            // see also 7736e440-4c6b-11ec-8c4d-b42e99f52061
            let x = match self {
                Storage::Shared => {
                    use public::ethereum_blocks as b;

                    b::table
                        .select((
                            b::hash,
                            b::number,
                            b::parent_hash,
                            sql::<Jsonb>("coalesce(data -> 'block', data)"),
                        ))
                        .filter(b::network_name.eq(chain))
                        .filter(b::hash.eq(any(Vec::from_iter(
                            hashes.iter().map(|h| format!("{:x}", h)),
                        ))))
                        .load::<(BlockHash, i64, BlockHash, json::Value)>(conn)
                }
                Storage::Private(Schema { blocks, .. }) => blocks
                    .table()
                    .select((
                        blocks.hash(),
                        blocks.number(),
                        blocks.parent_hash(),
                        sql::<Jsonb>("coalesce(data -> 'block', data)"),
                    ))
                    .filter(
                        blocks
                            .hash()
                            .eq(any(Vec::from_iter(hashes.iter().map(|h| h.as_slice())))),
                    )
                    .load::<(BlockHash, i64, BlockHash, json::Value)>(conn),
            }?;
            Ok(x.into_iter()
                .map(|(hash, nr, parent, data)| {
                    JsonBlock::new(BlockPtr::new(hash, nr as i32), parent, Some(data))
                })
                .collect())
        }

        pub(super) fn block_hashes_by_block_number(
            &self,
            conn: &PgConnection,
            chain: &str,
            number: BlockNumber,
        ) -> Result<Vec<BlockHash>, Error> {
            match self {
                Storage::Shared => {
                    use public::ethereum_blocks as b;

                    b::table
                        .select(b::hash)
                        .filter(b::network_name.eq(&chain))
                        .filter(b::number.eq(number as i64))
                        .get_results::<String>(conn)?
                        .into_iter()
                        .map(|h| h.parse())
                        .collect::<Result<Vec<BlockHash>, _>>()
                        .map_err(Error::from)
                }
                Storage::Private(Schema { blocks, .. }) => Ok(blocks
                    .table()
                    .select(blocks.hash())
                    .filter(blocks.number().eq(number as i64))
                    .get_results::<Vec<u8>>(conn)?
                    .into_iter()
                    .map(BlockHash::from)
                    .collect::<Vec<BlockHash>>()),
            }
        }

        pub(super) fn confirm_block_hash(
            &self,
            conn: &PgConnection,
            chain: &str,
            number: BlockNumber,
            hash: &BlockHash,
        ) -> Result<usize, Error> {
            let number = number as i64;

            match self {
                Storage::Shared => {
                    use public::ethereum_blocks as b;

                    let hash = format!("{:x}", hash);
                    diesel::delete(b::table)
                        .filter(b::network_name.eq(chain))
                        .filter(b::number.eq(number))
                        .filter(b::hash.ne(&hash))
                        .execute(conn)
                        .map_err(Error::from)
                }
                Storage::Private(Schema { blocks, .. }) => {
                    let query = format!(
                        "delete from {} where number = $1 and hash != $2",
                        blocks.qname
                    );
                    sql_query(query)
                        .bind::<BigInt, _>(number)
                        .bind::<Bytea, _>(hash.as_slice())
                        .execute(conn)
                        .map_err(Error::from)
                }
            }
        }

        /// timestamp's representation depends the blockchain::Block implementation, on
        /// ethereum this is a U256 but on different chains it will most likely be different.
        pub(super) fn block_number(
            &self,
            conn: &PgConnection,
            hash: &BlockHash,
        ) -> Result<Option<(BlockNumber, Option<u64>)>, StoreError> {
            const TIMESTAMP_QUERY: &str =
                "coalesce(data->'block'->>'timestamp', data->>'timestamp')";

            let number = match self {
                Storage::Shared => {
                    use public::ethereum_blocks as b;

                    b::table
                        .select((b::number, sql(TIMESTAMP_QUERY)))
                        .filter(b::hash.eq(format!("{:x}", hash)))
                        .first::<(i64, Option<String>)>(conn)
                        .optional()?
                }
                Storage::Private(Schema { blocks, .. }) => blocks
                    .table()
                    .select((blocks.number(), sql(TIMESTAMP_QUERY)))
                    .filter(blocks.hash().eq(hash.as_slice()))
                    .first::<(i64, Option<String>)>(conn)
                    .optional()?,
            };

            match number {
                None => Ok(None),
                Some((number, ts)) => {
                    let number = BlockNumber::try_from(number)
                        .map_err(|e| StoreError::QueryExecutionError(e.to_string()))?;
                    Ok(Some((number, crate::chain_store::try_parse_timestamp(ts)?)))
                }
            }
        }

        /// Find the first block that is missing from the database needed to
        /// complete the chain from block `hash` to the block with number
        /// `first_block`.
        pub(super) fn missing_parent(
            &self,
            conn: &PgConnection,
            chain: &str,
            first_block: i64,
            hash: H256,
            genesis: H256,
        ) -> Result<Option<H256>, Error> {
            match self {
                Storage::Shared => {
                    // We recursively build a temp table 'chain' containing the hash and
                    // parent_hash of blocks to check. The 'last' value is used to stop
                    // the recursion and is true if one of these conditions is true:
                    //   * we are missing a parent block
                    //   * we checked the required number of blocks
                    //   * we checked the genesis block
                    const MISSING_PARENT_SQL: &str = "
            with recursive chain(hash, parent_hash, last) as (
                -- base case: look at the head candidate block
                select b.hash, b.parent_hash, false
                  from ethereum_blocks b
                 where b.network_name = $1
                   and b.hash = $2
                   and b.hash != $3
                union all
                -- recursion step: add a block whose hash is the latest parent_hash
                -- on chain
                select chain.parent_hash,
                       b.parent_hash,
                       coalesce(b.parent_hash is null
                             or b.number <= $4
                             or b.hash = $3, true)
                  from chain left outer join ethereum_blocks b
                              on chain.parent_hash = b.hash
                             and b.network_name = $1
                 where not chain.last)
             select hash
               from chain
              where chain.parent_hash is null;
            ";

                    let hash = format!("{:x}", hash);
                    let genesis = format!("{:x}", genesis);
                    let missing = sql_query(MISSING_PARENT_SQL)
                        .bind::<Text, _>(chain)
                        .bind::<Text, _>(&hash)
                        .bind::<Text, _>(&genesis)
                        .bind::<BigInt, _>(first_block)
                        .load::<BlockHashText>(conn)?;

                    let missing = match missing.len() {
                        0 => None,
                        1 => Some(missing[0].hash.parse()?),
                        _ => {
                            unreachable!("the query can only return no or one row");
                        }
                    };
                    Ok(missing)
                }
                Storage::Private(Schema { blocks, .. }) => {
                    // This is the same as `MISSING_PARENT_SQL` above except that
                    // the blocks table has a different name and that it does
                    // not have a `network_name` column
                    let query = format!(
                        "
            with recursive chain(hash, parent_hash, last) as (
                -- base case: look at the head candidate block
                select b.hash, b.parent_hash, false
                  from {qname} b
                 where b.hash = $1
                   and b.hash != $2
                union all
                -- recursion step: add a block whose hash is the latest parent_hash
                -- on chain
                select chain.parent_hash,
                       b.parent_hash,
                       coalesce(b.parent_hash is null
                             or b.number <= $3
                             or b.hash = $2, true)
                  from chain left outer join {qname} b
                              on chain.parent_hash = b.hash
                 where not chain.last)
             select hash
               from chain
              where chain.parent_hash is null;
            ",
                        qname = blocks.qname
                    );

                    let missing = sql_query(query)
                        .bind::<Bytea, _>(hash.as_bytes())
                        .bind::<Bytea, _>(genesis.as_bytes())
                        .bind::<BigInt, _>(first_block)
                        .load::<BlockHashBytea>(conn)?;

                    let missing = match missing.len() {
                        0 => None,
                        1 => Some(h256_from_bytes(&missing[0].hash)?),
                        _ => {
                            unreachable!("the query can only return no or one row")
                        }
                    };
                    Ok(missing)
                }
            }
        }

        /// Return the best candidate for the new chain head if there is a block
        /// with a higher block number than the current chain head. The returned
        /// value if the hash and number of the candidate and the genesis block
        /// hash for the chain
        pub(super) fn chain_head_candidate(
            &self,
            conn: &PgConnection,
            chain: &str,
        ) -> Result<Option<BlockPtr>, Error> {
            use public::ethereum_networks as n;

            let head = n::table
                .filter(n::name.eq(chain))
                .select(n::head_block_number)
                .first::<Option<i64>>(conn)?
                .unwrap_or(-1);

            match self {
                Storage::Shared => {
                    use public::ethereum_blocks as b;
                    b::table
                        .filter(b::network_name.eq(chain))
                        .filter(b::number.gt(head))
                        .order_by((b::number.desc(), b::hash))
                        .select((b::hash, b::number))
                        .first::<(String, i64)>(conn)
                        .optional()?
                        .map(|(hash, number)| BlockPtr::try_from((hash.as_str(), number)))
                        .transpose()
                }
                Storage::Private(Schema { blocks, .. }) => blocks
                    .table()
                    .filter(blocks.number().gt(head))
                    .order_by((blocks.number().desc(), blocks.hash()))
                    .select((blocks.hash(), blocks.number()))
                    .first::<(Vec<u8>, i64)>(conn)
                    .optional()?
                    .map(|(hash, number)| BlockPtr::try_from((hash.as_slice(), number)))
                    .transpose(),
            }
        }

        pub(super) fn ancestor_block(
            &self,
            conn: &PgConnection,
            block_ptr: BlockPtr,
            offset: BlockNumber,
        ) -> Result<Option<(json::Value, BlockPtr)>, Error> {
            let data_and_hash = match self {
                Storage::Shared => {
                    const ANCESTOR_SQL: &str = "
        with recursive ancestors(block_hash, block_offset) as (
            values ($1, 0)
            union all
            select b.parent_hash, a.block_offset+1
              from ancestors a, ethereum_blocks b
             where a.block_hash = b.hash
               and a.block_offset < $2
        )
        select a.block_hash as hash
          from ancestors a
         where a.block_offset = $2;";

                    let hash = sql_query(ANCESTOR_SQL)
                        .bind::<Text, _>(block_ptr.hash_hex())
                        .bind::<BigInt, _>(offset as i64)
                        .get_result::<BlockHashText>(conn)
                        .optional()?;

                    use public::ethereum_blocks as b;

                    match hash {
                        None => None,
                        Some(hash) => Some((
                            b::table
                                .filter(b::hash.eq(&hash.hash))
                                .select(b::data)
                                .first::<json::Value>(conn)?,
                            BlockHash::from_str(&hash.hash)?,
                        )),
                    }
                }
                Storage::Private(Schema { blocks, .. }) => {
                    // Same as ANCESTOR_SQL except for the table name
                    let query = format!(
                        "
        with recursive ancestors(block_hash, block_offset) as (
            values ($1, 0)
            union all
            select b.parent_hash, a.block_offset+1
              from ancestors a, {} b
             where a.block_hash = b.hash
               and a.block_offset < $2
        )
        select a.block_hash as hash
          from ancestors a
         where a.block_offset = $2;",
                        blocks.qname
                    );

                    let hash = sql_query(query)
                        .bind::<Bytea, _>(block_ptr.hash_slice())
                        .bind::<BigInt, _>(offset as i64)
                        .get_result::<BlockHashBytea>(conn)
                        .optional()?;
                    match hash {
                        None => None,
                        Some(hash) => Some((
                            blocks
                                .table()
                                .filter(blocks.hash().eq(&hash.hash))
                                .select(blocks.data())
                                .first::<json::Value>(conn)?,
                            BlockHash::from(hash.hash),
                        )),
                    }
                }
            };

            // We need to deal with chain stores where some entries have a
            // toplevel 'blocks' field and others directly contain what
            // would be in the 'blocks' field. Make sure the value we return
            // has a 'block' entry
            //
            // see also 7736e440-4c6b-11ec-8c4d-b42e99f52061
            let data_and_ptr = {
                use graph::prelude::serde_json::json;

                data_and_hash.map(|(data, hash)| {
                    (
                        match data.get("block") {
                            Some(_) => data,
                            None => json!({ "block": data, "transaction_receipts": [] }),
                        },
                        BlockPtr::new(hash, block_ptr.number - offset),
                    )
                })
            };
            Ok(data_and_ptr)
        }

        pub(super) fn delete_blocks_before(
            &self,
            conn: &PgConnection,
            chain: &str,
            block: i64,
        ) -> Result<usize, Error> {
            match self {
                Storage::Shared => {
                    use public::ethereum_blocks as b;

                    diesel::delete(b::table)
                        .filter(b::network_name.eq(chain))
                        .filter(b::number.lt(block))
                        .filter(b::number.gt(0))
                        .execute(conn)
                        .map_err(Error::from)
                }
                Storage::Private(Schema { blocks, .. }) => {
                    let query = format!(
                        "delete from {} where number < $1 and number > 0",
                        blocks.qname
                    );
                    sql_query(query)
                        .bind::<BigInt, _>(block)
                        .execute(conn)
                        .map_err(Error::from)
                }
            }
        }

        pub(super) fn delete_blocks_by_hash(
            &self,
            conn: &PgConnection,
            chain: &str,
            block_hashes: &[&H256],
        ) -> Result<usize, Error> {
            use diesel::dsl::any;
            match self {
                Storage::Shared => {
                    use public::ethereum_blocks as b;

                    let hashes: Vec<String> = block_hashes
                        .iter()
                        .map(|hash| format!("{hash:x}"))
                        .collect();

                    diesel::delete(b::table)
                        .filter(b::network_name.eq(chain))
                        .filter(b::hash.eq(any(hashes)))
                        .filter(b::number.gt(0)) // keep genesis
                        .execute(conn)
                        .map_err(Error::from)
                }
                Storage::Private(Schema { blocks, .. }) => {
                    let query = format!(
                        "delete from {} where hash = any($1) and number > 0",
                        blocks.qname
                    );

                    let hashes: Vec<&[u8]> =
                        block_hashes.iter().map(|hash| hash.as_bytes()).collect();

                    sql_query(query)
                        .bind::<Array<Bytea>, _>(hashes)
                        .execute(conn)
                        .map_err(Error::from)
                }
            }
        }

        pub(super) fn get_call_and_access(
            &self,
            conn: &PgConnection,
            id: &[u8],
        ) -> Result<Option<(Vec<u8>, bool)>, Error> {
            match self {
                Storage::Shared => {
                    use public::eth_call_cache as cache;
                    use public::eth_call_meta as meta;

                    cache::table
                        .find::<&[u8]>(id.as_ref())
                        .inner_join(meta::table)
                        .select((
                            cache::return_value,
                            sql("CURRENT_DATE > eth_call_meta.accessed_at"),
                        ))
                        .get_result(conn)
                        .optional()
                        .map_err(Error::from)
                }
                Storage::Private(Schema {
                    call_cache,
                    call_meta,
                    ..
                }) => call_cache
                    .table()
                    .inner_join(
                        call_meta.table().on(call_meta
                            .contract_address()
                            .eq(call_cache.contract_address())),
                    )
                    .filter(call_cache.id().eq(id))
                    .select((
                        call_cache.return_value(),
                        sql(&format!(
                            "CURRENT_DATE > {}.{}",
                            CallMetaTable::TABLE_NAME,
                            CallMetaTable::ACCESSED_AT
                        )),
                    ))
                    .first(conn)
                    .optional()
                    .map_err(Error::from),
            }
        }

        pub(super) fn get_calls_in_block(
            &self,
            conn: &PgConnection,
            block_ptr: BlockPtr,
        ) -> Result<Vec<CachedEthereumCall>, Error> {
            let block_num = block_ptr.block_number();

            let rows = match self {
                Storage::Shared => {
                    use public::eth_call_cache as cache;

                    cache::table
                        .select((cache::id, cache::return_value, cache::contract_address))
                        .filter(cache::block_number.eq(block_num))
                        .order(cache::contract_address)
                        .get_results::<(Vec<u8>, Vec<u8>, Vec<u8>)>(conn)?
                }
                Storage::Private(Schema { call_cache, .. }) => call_cache
                    .table()
                    .select((
                        call_cache.id(),
                        call_cache.return_value(),
                        call_cache.contract_address(),
                    ))
                    .filter(call_cache.block_number().eq(block_num as i64))
                    .order(call_cache.contract_address())
                    .get_results::<(Vec<u8>, Vec<u8>, Vec<u8>)>(conn)?,
            };

            Ok(rows
                .into_iter()
                .map(|row| CachedEthereumCall {
                    blake3_id: row.0,
                    block_ptr: block_ptr.clone(),
                    contract_address: H160::from_slice(&row.2[..]),
                    return_value: row.1,
                })
                .collect())
        }

        pub(super) fn clear_call_cache(
            &self,
            conn: &PgConnection,
            head: BlockNumber,
            from: BlockNumber,
            to: BlockNumber,
        ) -> Result<(), Error> {
            if from <= 0 && to >= head {
                // We are removing the entire cache. Truncating is much
                // faster in that case
                self.truncate_call_cache(conn)?;
                return Ok(());
            }
            match self {
                Storage::Shared => {
                    use public::eth_call_cache as cache;
                    diesel::delete(
                        cache::table
                            .filter(cache::block_number.ge(from))
                            .filter(cache::block_number.le(to)),
                    )
                    .execute(conn)
                    .map_err(Error::from)?;
                    Ok(())
                }
                Storage::Private(Schema { call_cache, .. }) => {
                    // Because they are dynamically defined, our private call cache tables can't
                    // implement all the required traits for deletion. This means we can't use Diesel
                    // DSL with them and must rely on the `sql_query` function instead.
                    let query = format!(
                        "delete from {} where block_number >= $1 and block_number <= $2",
                        call_cache.qname
                    );
                    sql_query(query)
                        .bind::<Integer, _>(from)
                        .bind::<Integer, _>(to)
                        .execute(conn)
                        .map_err(Error::from)
                        .map(|_| ())
                }
            }
        }

        pub(super) fn update_accessed_at(
            &self,
            conn: &PgConnection,
            contract_address: &[u8],
        ) -> Result<(), Error> {
            let result = match self {
                Storage::Shared => {
                    use public::eth_call_meta as meta;

                    update(meta::table.find::<&[u8]>(contract_address.as_ref()))
                        .set(meta::accessed_at.eq(sql("CURRENT_DATE")))
                        .execute(conn)
                }
                Storage::Private(Schema { call_meta, .. }) => {
                    let query = format!(
                        "update {} set accessed_at = CURRENT_DATE where contract_address = $1",
                        call_meta.qname
                    );
                    sql_query(query)
                        .bind::<Bytea, _>(contract_address)
                        .execute(conn)
                }
            };
            result.map(|_| ()).map_err(Error::from)
        }

        pub(super) fn set_call(
            &self,
            conn: &PgConnection,
            id: &[u8],
            contract_address: &[u8],
            block_number: i32,
            return_value: &[u8],
        ) -> Result<(), Error> {
            let result = match self {
                Storage::Shared => {
                    use public::eth_call_cache as cache;
                    use public::eth_call_meta as meta;

                    insert_into(cache::table)
                        .values((
                            cache::id.eq(id),
                            cache::contract_address.eq(contract_address),
                            cache::block_number.eq(block_number),
                            cache::return_value.eq(return_value),
                        ))
                        .on_conflict_do_nothing()
                        .execute(conn)?;

                    // See comment in the Private branch for why the
                    // raciness of this check is ok
                    let update_meta = meta::table
                        .filter(meta::contract_address.eq(contract_address))
                        .select(sql("accessed_at < current_date"))
                        .first::<bool>(conn)
                        .optional()?
                        .unwrap_or(true);
                    if update_meta {
                        let accessed_at = meta::accessed_at.eq(sql("CURRENT_DATE"));
                        insert_into(meta::table)
                            .values((
                                meta::contract_address.eq(contract_address),
                                accessed_at.clone(),
                            ))
                            .on_conflict(meta::contract_address)
                            .do_update()
                            .set(accessed_at)
                            // TODO: Add a where clause similar to the Private
                            // branch to avoid unnecessary updates (not entirely
                            // trivial with diesel)
                            .execute(conn)
                    } else {
                        Ok(0)
                    }
                }
                Storage::Private(Schema {
                    call_cache,
                    call_meta,
                    ..
                }) => {
                    let query = format!(
                        "insert into {}(id, contract_address, block_number, return_value) \
                         values ($1, $2, $3, $4) on conflict do nothing",
                        call_cache.qname
                    );
                    sql_query(query)
                        .bind::<Bytea, _>(id)
                        .bind::<Bytea, _>(contract_address)
                        .bind::<Integer, _>(block_number)
                        .bind::<Bytea, _>(return_value)
                        .execute(conn)?;

                    // Check whether we need to update `call_meta`. The
                    // check is racy, since an update can happen between the
                    // check and the insert below, but that's fine. We can
                    // tolerate a small number of redundant updates, but
                    // will still catch the majority of cases where an
                    // update is not needed
                    let update_meta = call_meta
                        .table()
                        .filter(call_meta.contract_address().eq(contract_address))
                        .select(sql("accessed_at < current_date"))
                        .first::<bool>(conn)
                        .optional()?
                        .unwrap_or(true);

                    if update_meta {
                        let query = format!(
                            "insert into {}(contract_address, accessed_at) \
                         values ($1, CURRENT_DATE) \
                         on conflict(contract_address)
                         do update set accessed_at = CURRENT_DATE \
                                 where excluded.accessed_at < CURRENT_DATE",
                            call_meta.qname
                        );
                        sql_query(query)
                            .bind::<Bytea, _>(contract_address)
                            .execute(conn)
                    } else {
                        Ok(0)
                    }
                }
            };
            result.map(|_| ()).map_err(Error::from)
        }

        #[cfg(debug_assertions)]
        // used by `super::set_chain` for test support
        pub(super) fn remove_chain(&self, conn: &PgConnection, chain_name: &str) {
            match self {
                Storage::Shared => {
                    use public::eth_call_cache as c;
                    use public::eth_call_meta as m;
                    use public::ethereum_blocks as b;

                    diesel::delete(b::table.filter(b::network_name.eq(chain_name)))
                        .execute(conn)
                        .expect("Failed to delete ethereum_blocks");
                    // We don't have a good way to clean out the call cache
                    // per chain; just nuke everything
                    diesel::delete(c::table).execute(conn).unwrap();
                    diesel::delete(m::table).execute(conn).unwrap();
                }
                Storage::Private(Schema {
                    blocks,
                    call_meta,
                    call_cache,
                    ..
                }) => {
                    for qname in &[&blocks.qname, &call_meta.qname, &call_cache.qname] {
                        let query = format!("delete from {}", qname);
                        sql_query(query)
                            .execute(conn)
                            .unwrap_or_else(|_| panic!("Failed to delete {}", qname));
                    }
                }
            }
        }

        /// Queries the database for all the transaction receipts in a given block.
        pub(crate) fn find_transaction_receipts_in_block(
            &self,
            conn: &PgConnection,
            block_hash: H256,
        ) -> anyhow::Result<Vec<LightTransactionReceipt>> {
            let query = sql_query(format!(
                "
select
    ethereum_hex_to_bytea(receipt ->> 'transactionHash') as transaction_hash,
    ethereum_hex_to_bytea(receipt ->> 'transactionIndex') as transaction_index,
    ethereum_hex_to_bytea(receipt ->> 'blockHash') as block_hash,
    ethereum_hex_to_bytea(receipt ->> 'blockNumber') as block_number,
    ethereum_hex_to_bytea(receipt ->> 'gasUsed') as gas_used,
    ethereum_hex_to_bytea(receipt ->> 'status') as status
from (
    select
        jsonb_array_elements(data -> 'transaction_receipts') as receipt
    from
        {blocks_table_name}
    where hash = $1) as temp;
",
                blocks_table_name = self.blocks_table()
            ));

            let query_results: Result<Vec<RawTransactionReceipt>, diesel::result::Error> = {
                // The `hash` column has different types between the `public.ethereum_blocks` and the
                // `chain*.blocks` tables, so we must check which one is being queried to bind the
                // `block_hash` parameter to the correct type
                match self {
                    Storage::Shared => query
                        .bind::<Text, _>(format!("{:x}", block_hash))
                        .get_results(conn),
                    Storage::Private(_) => query
                        .bind::<Binary, _>(block_hash.as_bytes())
                        .get_results(conn),
                }
            };
            query_results
                .map_err(|error| {
                    anyhow::anyhow!(
                        "Error fetching transaction receipt from database: {}",
                        error
                    )
                })?
                .into_iter()
                .map(LightTransactionReceipt::try_from)
                .collect()
        }
    }
}

#[derive(Debug)]
pub struct ChainStoreMetrics {
    chain_head_cache_size: Box<GaugeVec>,
    chain_head_cache_oldest_block_num: Box<GaugeVec>,
    chain_head_cache_latest_block_num: Box<GaugeVec>,
    chain_head_cache_hits: Box<CounterVec>,
    chain_head_cache_misses: Box<CounterVec>,
}

impl ChainStoreMetrics {
    pub fn new(registry: Arc<MetricsRegistry>) -> Self {
        let chain_head_cache_size = registry
            .new_gauge_vec(
                "chain_head_cache_num_blocks",
                "Number of blocks in the chain head cache",
                vec!["network".to_string()],
            )
            .expect("Can't register the gauge");
        let chain_head_cache_oldest_block_num = registry
            .new_gauge_vec(
                "chain_head_cache_oldest_block",
                "Block number of the oldest block currently present in the chain head cache",
                vec!["network".to_string()],
            )
            .expect("Can't register the gauge");
        let chain_head_cache_latest_block_num = registry
            .new_gauge_vec(
                "chain_head_cache_latest_block",
                "Block number of the latest block currently present in the chain head cache",
                vec!["network".to_string()],
            )
            .expect("Can't register the gauge");

        let chain_head_cache_hits = registry
            .new_counter_vec(
                "chain_head_cache_hits",
                "Number of times the chain head cache was hit",
                vec!["network".to_string()],
            )
            .expect("Can't register the counter");
        let chain_head_cache_misses = registry
            .new_counter_vec(
                "chain_head_cache_misses",
                "Number of times the chain head cache was missed",
                vec!["network".to_string()],
            )
            .expect("Can't register the counter");

        Self {
            chain_head_cache_size,
            chain_head_cache_oldest_block_num,
            chain_head_cache_latest_block_num,
            chain_head_cache_hits,
            chain_head_cache_misses,
        }
    }

    pub fn add_block(&self, network: &str) {
        self.chain_head_cache_size
            .with_label_values(&[network])
            .inc();
    }

    pub fn remove_block(&self, network: &str) {
        self.chain_head_cache_size
            .with_label_values(&[network])
            .dec();
    }

    pub fn record_cache_hit(&self, network: &str) {
        self.chain_head_cache_hits
            .get_metric_with_label_values(&[network])
            .unwrap()
            .inc();
    }

    pub fn record_cache_miss(&self, network: &str) {
        self.chain_head_cache_misses
            .get_metric_with_label_values(&[network])
            .unwrap()
            .inc();
    }

    pub fn record_hit_and_miss(&self, network: &str, hits: usize, misses: usize) {
        self.chain_head_cache_hits
            .get_metric_with_label_values(&[network])
            .unwrap()
            .inc_by(hits as f64);
        self.chain_head_cache_misses
            .get_metric_with_label_values(&[network])
            .unwrap()
            .inc_by(misses as f64);
    }
}

pub struct ChainStore {
    pool: ConnectionPool,
    pub chain: String,
    pub(crate) storage: data::Storage,
    pub chain_identifier: ChainIdentifier,
    genesis_block_ptr: BlockPtr,
    status: ChainStatus,
    chain_head_update_sender: ChainHeadUpdateSender,
    // TODO: We currently only use this cache for
    // [`ChainStore::ancestor_block`], but it could very well be expanded to
    // also track the network's chain head and generally improve its hit rate.
    // It is, however, quite challenging to keep the cache perfectly consistent
    // with the database and to correctly implement invalidation. So, a
    // conservative approach is acceptable.
    recent_blocks_cache: RecentBlocksCache,
}

impl ChainStore {
    pub(crate) fn new(
        chain: String,
        storage: data::Storage,
        net_identifier: &ChainIdentifier,
        status: ChainStatus,
        chain_head_update_sender: ChainHeadUpdateSender,
        pool: ConnectionPool,
        recent_blocks_cache_capacity: usize,
        metrics: Arc<ChainStoreMetrics>,
    ) -> Self {
        let recent_blocks_cache =
            RecentBlocksCache::new(recent_blocks_cache_capacity, chain.clone(), metrics);
        ChainStore {
            pool,
            chain,
            storage,
            genesis_block_ptr: BlockPtr::new(net_identifier.genesis_block_hash.clone(), 0),
            status,
            chain_head_update_sender,
            chain_identifier: net_identifier.clone(),
            recent_blocks_cache,
        }
    }

    pub fn is_ingestible(&self) -> bool {
        matches!(self.status, ChainStatus::Ingestible)
    }

    fn get_conn(&self) -> Result<PooledConnection<ConnectionManager<PgConnection>>, Error> {
        self.pool.get().map_err(Error::from)
    }

    pub(crate) fn create(&self, ident: &ChainIdentifier) -> Result<(), Error> {
        use public::ethereum_networks::dsl::*;

        let conn = self.get_conn()?;
        conn.transaction(|| {
            insert_into(ethereum_networks)
                .values((
                    name.eq(&self.chain),
                    namespace.eq(&self.storage),
                    head_block_hash.eq::<Option<String>>(None),
                    head_block_number.eq::<Option<i64>>(None),
                    net_version.eq(&ident.net_version),
                    genesis_block_hash.eq(ident.genesis_block_hash.hash_hex()),
                ))
                .on_conflict(name)
                .do_nothing()
                .execute(&conn)?;
            self.storage.create(&conn)
        })?;

        Ok(())
    }

    pub(crate) fn drop_chain(&self) -> Result<(), Error> {
        use diesel::dsl::delete;
        use public::ethereum_networks as n;

        let conn = self.get_conn()?;
        conn.transaction(|| {
            self.storage.drop_storage(&conn, &self.chain)?;

            delete(n::table.filter(n::name.eq(&self.chain))).execute(&conn)?;
            Ok(())
        })
    }

    pub fn chain_head_pointers(
        conn: &PgConnection,
    ) -> Result<HashMap<String, BlockPtr>, StoreError> {
        use public::ethereum_networks as n;

        let pointers: Vec<(String, BlockPtr)> = n::table
            .select((n::name, n::head_block_hash, n::head_block_number))
            .load::<(String, Option<String>, Option<i64>)>(conn)?
            .into_iter()
            .filter_map(|(name, hash, number)| match (hash, number) {
                (Some(hash), Some(number)) => Some((name, hash, number)),
                _ => None,
            })
            .map(|(name, hash, number)| {
                BlockPtr::try_from((hash.as_str(), number)).map(|ptr| (name, ptr))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(HashMap::from_iter(pointers))
    }

    pub fn chain_head_block(&self, chain: &str) -> Result<Option<BlockNumber>, StoreError> {
        use public::ethereum_networks as n;

        let number: Option<i64> = n::table
            .filter(n::name.eq(chain))
            .select(n::head_block_number)
            .first::<Option<i64>>(&self.get_conn()?)
            .optional()?
            .flatten();

        number.map(|number| number.try_into()).transpose().map_err(
            |e: std::num::TryFromIntError| {
                constraint_violation!(
                    "head block number for {} is {:?} which does not fit into a u32: {}",
                    chain,
                    number,
                    e.to_string()
                )
            },
        )
    }

    /// Store the given chain as the blocks for the `network` set the
    /// network's genesis block to `genesis_hash`, and head block to
    /// `null`
    #[cfg(debug_assertions)]
    pub async fn set_chain(
        &self,
        genesis_hash: &str,
        chain: Vec<Arc<dyn Block>>,
    ) -> Vec<(BlockPtr, BlockHash)> {
        let conn = self.pool.get().expect("can get a database connection");

        self.storage.remove_chain(&conn, &self.chain);
        self.recent_blocks_cache.clear();

        for block in chain {
            self.upsert_block(block).await.expect("can upsert block");
        }

        use public::ethereum_networks as n;
        diesel::update(n::table.filter(n::name.eq(&self.chain)))
            .set((
                n::genesis_block_hash.eq(genesis_hash),
                n::head_block_hash.eq::<Option<&str>>(None),
                n::head_block_number.eq::<Option<i64>>(None),
            ))
            .execute(&conn)
            .unwrap();
        self.recent_blocks_cache.blocks()
    }

    pub fn delete_blocks(&self, block_hashes: &[&H256]) -> Result<usize, Error> {
        let conn = self.get_conn()?;
        self.storage
            .delete_blocks_by_hash(&conn, &self.chain, block_hashes)
    }

    pub fn cleanup_shallow_blocks(&self, lowest_block: i32) -> Result<(), StoreError> {
        let conn = self.get_conn()?;
        self.storage.cleanup_shallow_blocks(&conn, lowest_block)?;
        Ok(())
    }

    // remove_cursor delete the chain_store cursor and return true if it was present
    pub fn remove_cursor(&self, chain: &str) -> Result<Option<BlockNumber>, StoreError> {
        let conn = self.get_conn()?;
        self.storage.remove_cursor(&conn, chain)
    }

    pub fn truncate_block_cache(&self) -> Result<(), StoreError> {
        let conn = self.get_conn()?;
        self.storage.truncate_block_cache(&conn)?;
        Ok(())
    }
}

#[async_trait]
impl ChainStoreTrait for ChainStore {
    fn genesis_block_ptr(&self) -> Result<BlockPtr, Error> {
        Ok(self.genesis_block_ptr.clone())
    }

    async fn upsert_block(&self, block: Arc<dyn Block>) -> Result<(), Error> {
        // We should always have the parent block available to us at this point.
        if let Some(parent_hash) = block.parent_hash() {
            let block = JsonBlock::new(block.ptr(), parent_hash, block.data().ok());
            self.recent_blocks_cache.insert_block(block);
        }

        let pool = self.pool.clone();
        let network = self.chain.clone();
        let storage = self.storage.clone();
        pool.with_conn(move |conn, _| {
            conn.transaction(|| {
                storage
                    .upsert_block(conn, &network, block.as_ref(), true)
                    .map_err(CancelableError::from)
            })
        })
        .await
        .map_err(Error::from)
    }

    fn upsert_light_blocks(&self, blocks: &[&dyn Block]) -> Result<(), Error> {
        let conn = self.pool.get()?;
        for block in blocks {
            self.storage
                .upsert_block(&conn, &self.chain, *block, false)?;
        }
        Ok(())
    }

    async fn attempt_chain_head_update(
        self: Arc<Self>,
        ancestor_count: BlockNumber,
    ) -> Result<Option<H256>, Error> {
        use public::ethereum_networks as n;

        let (missing, ptr) = {
            let chain_store = self.clone();
            self.pool
                .with_conn(move |conn, _| {
                    let candidate = chain_store
                        .storage
                        .chain_head_candidate(conn, &chain_store.chain)
                        .map_err(CancelableError::from)?;
                    let (ptr, first_block) = match &candidate {
                        None => return Ok((None, None)),
                        Some(ptr) => (ptr, 0.max(ptr.number.saturating_sub(ancestor_count))),
                    };

                    match chain_store
                        .storage
                        .missing_parent(
                            conn,
                            &chain_store.chain,
                            first_block as i64,
                            ptr.hash_as_h256(),
                            chain_store.genesis_block_ptr.hash_as_h256(),
                        )
                        .map_err(CancelableError::from)?
                    {
                        Some(missing) => {
                            return Ok((Some(missing), None));
                        }
                        None => { /* we have a complete chain, no missing parents */ }
                    }

                    let hash = ptr.hash_hex();
                    let number = ptr.number as i64;

                    conn.transaction(
                        || -> Result<(Option<H256>, Option<(String, i64)>), StoreError> {
                            update(n::table.filter(n::name.eq(&chain_store.chain)))
                                .set((
                                    n::head_block_hash.eq(&hash),
                                    n::head_block_number.eq(number),
                                ))
                                .execute(conn)?;
                            Ok((None, Some((hash, number))))
                        },
                    )
                    .map_err(CancelableError::from)
                })
                .await?
        };
        if let Some((hash, number)) = ptr {
            self.chain_head_update_sender.send(&hash, number)?;
        }

        Ok(missing)
    }

    async fn chain_head_ptr(self: Arc<Self>) -> Result<Option<BlockPtr>, Error> {
        use public::ethereum_networks::dsl::*;

        Ok(self
            .cheap_clone()
            .pool
            .with_conn(move |conn, _| {
                ethereum_networks
                    .select((head_block_hash, head_block_number))
                    .filter(name.eq(&self.chain))
                    .load::<(Option<String>, Option<i64>)>(conn)
                    .map(|rows| {
                        rows.first()
                            .map(|(hash_opt, number_opt)| match (hash_opt, number_opt) {
                                (Some(hash), Some(number)) => Some(
                                    (
                                        // FIXME:
                                        //
                                        // workaround for arweave
                                        H256::from_slice(&hex::decode(hash).unwrap()[..32]),
                                        *number,
                                    )
                                        .into(),
                                ),
                                (None, None) => None,
                                _ => unreachable!(),
                            })
                            .and_then(|opt: Option<BlockPtr>| opt)
                    })
                    .map_err(|e| CancelableError::from(StoreError::from(e)))
            })
            .await?)
    }

    async fn cached_head_ptr(self: Arc<Self>) -> Result<Option<BlockPtr>, Error> {
        match self.recent_blocks_cache.chain_head_ptr() {
            Some(head) => Ok(Some(head)),
            None => self.chain_head_ptr().await,
        }
    }

    fn chain_head_cursor(&self) -> Result<Option<String>, Error> {
        use public::ethereum_networks::dsl::*;

        ethereum_networks
            .select(head_block_cursor)
            .filter(name.eq(&self.chain))
            .load::<Option<String>>(&*self.get_conn()?)
            .map(|rows| {
                rows.first()
                    .map(|cursor_opt| cursor_opt.as_ref().cloned())
                    .and_then(|opt| opt)
            })
            .map_err(Error::from)
    }

    async fn set_chain_head(
        self: Arc<Self>,
        block: Arc<dyn Block>,
        cursor: String,
    ) -> Result<(), Error> {
        use public::ethereum_networks as n;

        let pool = self.pool.clone();
        let network = self.chain.clone();
        let storage = self.storage.clone();

        let ptr = block.ptr();
        let hash = ptr.hash_hex();
        let number = ptr.number as i64; //block height

        //this will send an update via postgres, channel: chain_head_updates
        self.chain_head_update_sender.send(&hash, number)?;

        pool.with_conn(move |conn, _| {
            conn.transaction(|| -> Result<(), StoreError> {
                storage
                    .upsert_block(conn, &network, block.as_ref(), true)
                    .map_err(CancelableError::from)?;

                update(n::table.filter(n::name.eq(&self.chain)))
                    .set((
                        n::head_block_hash.eq(&hash),
                        n::head_block_number.eq(number),
                        n::head_block_cursor.eq(cursor),
                    ))
                    .execute(conn)?;

                Ok(())
            })
            .map_err(CancelableError::from)
        })
        .await?;

        Ok(())
    }

    fn blocks(&self, hashes: &[BlockHash]) -> Result<Vec<json::Value>, Error> {
        if ENV_VARS.store.disable_block_cache_for_lookup {
            let conn = self.get_conn()?;
            let values = self
                .storage
                .blocks(&conn, &self.chain, &hashes)?
                .into_iter()
                .filter_map(|block| block.data)
                .collect();
            Ok(values)
        } else {
            let cached = self.recent_blocks_cache.get_blocks_by_hash(hashes);
            let stored = if cached.len() < hashes.len() {
                let hashes = hashes
                    .iter()
                    .filter(|hash| cached.iter().find(|(ptr, _)| &ptr.hash == *hash).is_none())
                    .cloned()
                    .collect::<Vec<_>>();
                let conn = self.get_conn()?;
                let stored = self.storage.blocks(&conn, &self.chain, &hashes)?;
                for block in &stored {
                    self.recent_blocks_cache.insert_block(block.clone());
                }
                stored
            } else {
                Vec::new()
            };

            let mut result = cached.into_iter().map(|(_, data)| data).collect::<Vec<_>>();
            let stored = stored.into_iter().filter_map(|block| block.data);
            result.extend(stored);
            Ok(result)
        }
    }

    async fn ancestor_block(
        self: Arc<Self>,
        block_ptr: BlockPtr,
        offset: BlockNumber,
    ) -> Result<Option<json::Value>, Error> {
        ensure!(
            block_ptr.number >= offset,
            "block offset {} for block `{}` points to before genesis block",
            offset,
            block_ptr.hash_hex()
        );

        // Check the local cache first.
        if let Some(data) = self.recent_blocks_cache.get_ancestor(&block_ptr, offset) {
            return Ok(data.1);
        }

        let block_ptr_clone = block_ptr.clone();
        let chain_store = self.cheap_clone();
        Ok(self
            .pool
            .with_conn(move |conn, _| {
                chain_store
                    .storage
                    .ancestor_block(conn, block_ptr_clone, offset)
                    .map_err(StoreError::from)
                    .map_err(CancelableError::from)
            })
            .await?
            .map(|b| b.0))
    }

    fn cleanup_cached_blocks(
        &self,
        ancestor_count: BlockNumber,
    ) -> Result<Option<(BlockNumber, usize)>, Error> {
        use diesel::sql_types::Integer;

        #[derive(QueryableByName)]
        struct MinBlock {
            #[sql_type = "Integer"]
            block: i32,
        }

        self.recent_blocks_cache.clear();

        // Remove all blocks from the cache that are behind the slowest
        // subgraph's head block, but retain the genesis block. We stay
        // behind the slowest subgraph so that we do not interfere with its
        // syncing activity.
        // We also stay `ancestor_count` many blocks behind the head of the
        // chain since the block ingestor consults these blocks frequently
        //
        // Only consider active subgraphs that have not failed

        // This assumes that subgraph metadata and blocks are stored in the
        // same shard. We disallow setting GRAPH_ETHEREUM_CLEANUP_BLOCKS in
        // graph_node::config so that we only run this query when we know
        // it will work. Running this with a sharded store might remove
        // blocks that are still needed by deployments in other shard
        //
        // See 8b6ad0c64e244023ac20ced7897fe666

        let conn = self.get_conn()?;
        let query = "
            select coalesce(
                   least(a.block,
                        (select head_block_number::int - $1
                           from ethereum_networks
                          where name = $2)), -1)::int as block
              from (
                select min(d.latest_ethereum_block_number) as block
                  from subgraphs.subgraph_deployment d,
                       subgraphs.subgraph_deployment_assignment a,
                       deployment_schemas ds
                 where ds.subgraph = d.deployment
                   and a.id = d.id
                   and not d.failed
                   and ds.network = $2) a;";
        diesel::sql_query(query)
            .bind::<Integer, _>(ancestor_count)
            .bind::<Text, _>(&self.chain)
            .load::<MinBlock>(&conn)?
            .first()
            .map(|MinBlock { block }| {
                // If we could not determine a minimum block, the query
                // returns -1, and we should not do anything. We also guard
                // against removing the genesis block
                if *block > 0 {
                    self.storage
                        .delete_blocks_before(&conn, &self.chain, *block as i64)
                        .map(|rows| Some((*block, rows)))
                } else {
                    Ok(None)
                }
            })
            .unwrap_or(Ok(None))
            .map_err(Into::into)
    }

    fn block_hashes_by_block_number(&self, number: BlockNumber) -> Result<Vec<BlockHash>, Error> {
        let conn = self.get_conn()?;
        self.storage
            .block_hashes_by_block_number(&conn, &self.chain, number)
    }

    fn confirm_block_hash(&self, number: BlockNumber, hash: &BlockHash) -> Result<usize, Error> {
        let conn = self.get_conn()?;
        self.storage
            .confirm_block_hash(&conn, &self.chain, number, hash)
    }

    async fn block_number(
        &self,
        hash: &BlockHash,
    ) -> Result<Option<(String, BlockNumber, Option<u64>)>, StoreError> {
        let hash = hash.clone();
        let storage = self.storage.clone();
        let chain = self.chain.clone();
        self.pool
            .with_conn(move |conn, _| {
                storage
                    .block_number(conn, &hash)
                    .map(|opt| opt.map(|(number, timestamp)| (chain.clone(), number, timestamp)))
                    .map_err(|e| e.into())
            })
            .await
    }

    async fn clear_call_cache(&self, from: BlockNumber, to: BlockNumber) -> Result<(), Error> {
        let conn = self.get_conn()?;
        if let Some(head) = self.chain_head_block(&self.chain)? {
            self.storage.clear_call_cache(&conn, head, from, to)?;
        }
        Ok(())
    }

    async fn transaction_receipts_in_block(
        &self,
        block_hash: &H256,
    ) -> Result<Vec<LightTransactionReceipt>, StoreError> {
        let pool = self.pool.clone();
        let storage = self.storage.clone();
        let block_hash = *block_hash;
        pool.with_conn(move |conn, _| {
            storage
                .find_transaction_receipts_in_block(conn, block_hash)
                .map_err(|e| StoreError::from(e).into())
        })
        .await
    }
}

mod recent_blocks_cache {
    use super::*;
    use std::collections::BTreeMap;

    struct Inner {
        network: String,
        metrics: Arc<ChainStoreMetrics>,
        // A list of blocks by block number. The list has at most `capacity`
        // entries. If there are multiple writes for the same block number,
        // the last one wins. Note that because of NEAR, the block numbers
        // might have gaps.
        blocks: BTreeMap<BlockNumber, JsonBlock>,
        // We only store these many blocks.
        capacity: usize,
    }

    impl Inner {
        fn get_block_by_hash(&self, hash: &BlockHash) -> Option<(&BlockPtr, &json::Value)> {
            self.blocks
                .values()
                .find(|block| &block.ptr.hash == hash)
                .and_then(|block| block.data.as_ref().map(|data| (&block.ptr, data)))
        }

        fn get_ancestor(
            &self,
            child_ptr: &BlockPtr,
            offset: BlockNumber,
        ) -> Option<(&BlockPtr, Option<&json::Value>)> {
            let child = self.blocks.get(&child_ptr.number)?;
            if &child.ptr != child_ptr {
                return None;
            }
            let ancestor_block_number = child.ptr.number - offset;
            let mut child = child;
            for number in (ancestor_block_number..child_ptr.number).rev() {
                let parent = self.blocks.get(&number)?;
                if child.parent_hash != parent.ptr.hash {
                    return None;
                }
                child = parent;
            }
            Some((&child.ptr, child.data.as_ref()))
        }

        fn chain_head(&self) -> Option<&BlockPtr> {
            self.blocks.last_key_value().map(|b| &b.1.ptr)
        }

        fn earliest_block(&self) -> Option<&JsonBlock> {
            self.blocks.first_key_value().map(|b| b.1)
        }

        fn evict_if_necessary(&mut self) {
            while self.blocks.len() > self.capacity {
                self.blocks.pop_first();
            }
        }

        fn update_write_metrics(&self) {
            self.metrics
                .chain_head_cache_size
                .get_metric_with_label_values(&[&self.network])
                .unwrap()
                .set(self.blocks.len() as f64);

            self.metrics
                .chain_head_cache_oldest_block_num
                .get_metric_with_label_values(&[&self.network])
                .unwrap()
                .set(self.earliest_block().map(|b| b.ptr.number).unwrap_or(0) as f64);

            self.metrics
                .chain_head_cache_latest_block_num
                .get_metric_with_label_values(&[&self.network])
                .unwrap()
                .set(self.chain_head().map(|b| b.number).unwrap_or(0) as f64);
        }

        fn insert_block(&mut self, block: JsonBlock) {
            self.blocks.insert(block.ptr.number, block);
            self.evict_if_necessary();
        }
    }

    /// We cache the most recent blocks in memory to avoid overloading the
    /// database with unnecessary queries close to the chain head. We invalidate
    /// blocks whenever the chain head advances.
    pub struct RecentBlocksCache {
        // We protect everything with a global `RwLock` to avoid data races. Ugly...
        inner: RwLock<Inner>,
    }

    impl RecentBlocksCache {
        pub fn new(capacity: usize, network: String, metrics: Arc<ChainStoreMetrics>) -> Self {
            RecentBlocksCache {
                inner: RwLock::new(Inner {
                    network,
                    metrics,
                    blocks: BTreeMap::new(),
                    capacity,
                }),
            }
        }

        pub fn chain_head_ptr(&self) -> Option<BlockPtr> {
            let inner = self.inner.read();
            inner.chain_head().cloned()
        }

        pub fn clear(&self) {
            self.inner.write().blocks.clear();
            self.inner.read().update_write_metrics();
        }

        pub fn get_ancestor(
            &self,
            child: &BlockPtr,
            offset: BlockNumber,
        ) -> Option<(BlockPtr, Option<json::Value>)> {
            let block_opt = self
                .inner
                .read()
                .get_ancestor(child, offset)
                .map(|b| (b.0.clone(), b.1.cloned()));

            let inner = self.inner.read();
            if block_opt.is_some() {
                inner.metrics.record_cache_hit(&inner.network);
            } else {
                inner.metrics.record_cache_miss(&inner.network);
            }

            block_opt
        }

        pub fn get_blocks_by_hash(&self, hashes: &[BlockHash]) -> Vec<(BlockPtr, json::Value)> {
            let inner = self.inner.read();
            let blocks: Vec<_> = hashes
                .iter()
                .filter_map(|hash| inner.get_block_by_hash(hash))
                .map(|(ptr, value)| (ptr.clone(), value.clone()))
                .collect();
            inner.metrics.record_hit_and_miss(
                &inner.network,
                blocks.len(),
                hashes.len() - blocks.len(),
            );
            blocks
        }

        /// Tentatively caches the `ancestor` of a [`BlockPtr`] (`child`), together with
        /// its associated `data`. Note that for this to work, `child` must be
        /// in the cache already. The first block in the cache should be
        /// inserted via [`RecentBlocksCache::set_chain_head`].
        pub(super) fn insert_block(&self, block: JsonBlock) {
            self.inner.write().insert_block(block);
            self.inner.read().update_write_metrics();
        }

        #[cfg(debug_assertions)]
        pub fn blocks(&self) -> Vec<(BlockPtr, BlockHash)> {
            self.inner
                .read()
                .blocks
                .values()
                .map(|block| (block.ptr.clone(), block.parent_hash.clone()))
                .collect()
        }
    }
}

fn try_parse_timestamp(ts: Option<String>) -> Result<Option<u64>, StoreError> {
    let ts = match ts {
        Some(str) => str,
        None => return Ok(None),
    };

    let (radix, idx) = if ts.starts_with("0x") {
        (16, 2)
    } else {
        (10, 0)
    };

    u64::from_str_radix(&ts[idx..], radix)
        .map_err(|err| {
            StoreError::QueryExecutionError(format!(
                "unexpected timestamp format {}, err: {}",
                ts, err
            ))
        })
        .map(Some)
}

impl EthereumCallCache for ChainStore {
    fn get_call(
        &self,
        contract_address: ethabi::Address,
        encoded_call: &[u8],
        block: BlockPtr,
    ) -> Result<Option<Vec<u8>>, Error> {
        let id = contract_call_id(&contract_address, encoded_call, &block);
        let conn = &*self.get_conn()?;
        if let Some(call_output) = conn.transaction::<_, Error, _>(|| {
            if let Some((return_value, update_accessed_at)) =
                self.storage.get_call_and_access(conn, id.as_ref())?
            {
                if update_accessed_at {
                    self.storage
                        .update_accessed_at(conn, contract_address.as_ref())?;
                }
                Ok(Some(return_value))
            } else {
                Ok(None)
            }
        })? {
            Ok(Some(call_output))
        } else {
            Ok(None)
        }
    }

    fn get_calls_in_block(&self, block: BlockPtr) -> Result<Vec<CachedEthereumCall>, Error> {
        let conn = &*self.get_conn()?;
        conn.transaction::<_, Error, _>(|| self.storage.get_calls_in_block(conn, block))
    }

    fn set_call(
        &self,
        contract_address: ethabi::Address,
        encoded_call: &[u8],
        block: BlockPtr,
        return_value: &[u8],
    ) -> Result<(), Error> {
        let id = contract_call_id(&contract_address, encoded_call, &block);
        let conn = &*self.get_conn()?;
        conn.transaction(|| {
            self.storage.set_call(
                conn,
                id.as_ref(),
                contract_address.as_ref(),
                block.number,
                return_value,
            )
        })
    }
}

/// The id is the hashed encoded_call + contract_address + block hash to uniquely identify the call.
/// 256 bits of output, and therefore 128 bits of security against collisions, are needed since this
/// could be targeted by a birthday attack.
fn contract_call_id(
    contract_address: &ethabi::Address,
    encoded_call: &[u8],
    block: &BlockPtr,
) -> [u8; 32] {
    let mut hash = blake3::Hasher::new();
    hash.update(encoded_call);
    hash.update(contract_address.as_ref());
    hash.update(block.hash_slice());
    *hash.finalize().as_bytes()
}
