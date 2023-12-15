//! SQL queries to load dynamic data sources

use diesel::{
    delete,
    dsl::{count, sql},
    prelude::{ExpressionMethods, QueryDsl, RunQueryDsl},
    sql_query,
    sql_types::{Integer, Text},
};
use diesel::{insert_into, pg::PgConnection};

use graph::{
    components::store::{write, StoredDynamicDataSource},
    constraint_violation,
    data_source::CausalityRegion,
    prelude::{
        bigdecimal::ToPrimitive, serde_json, BigDecimal, BlockNumber, DeploymentHash, StoreError,
    },
};

use crate::connection_pool::ForeignServer;
use crate::primary::Site;

table! {
    subgraphs.dynamic_ethereum_contract_data_source (vid) {
        vid -> BigInt,
        name -> Text,
        address -> Binary,
        abi -> Text,
        start_block -> Integer,
        // Never read
        ethereum_block_hash -> Binary,
        ethereum_block_number -> Numeric,
        deployment -> Text,
        context -> Nullable<Text>,
    }
}

pub(super) fn load(
    conn: &PgConnection,
    id: &str,
    block: BlockNumber,
    manifest_idx_and_name: Vec<(u32, String)>,
) -> Result<Vec<StoredDynamicDataSource>, StoreError> {
    use dynamic_ethereum_contract_data_source as decds;

    // Query to load the data sources. Ordering by the creation block and `vid` makes sure they are
    // in insertion order which is important for the correctness of reverts and the execution order
    // of triggers. See also 8f1bca33-d3b7-4035-affc-fd6161a12448.
    let dds: Vec<_> = decds::table
        .filter(decds::deployment.eq(id))
        .select((
            decds::vid,
            decds::name,
            decds::context,
            decds::address,
            decds::ethereum_block_number,
        ))
        .filter(decds::ethereum_block_number.le(sql(&format!("{}::numeric", block))))
        .order_by((decds::ethereum_block_number, decds::vid))
        .load::<(i64, String, Option<String>, Vec<u8>, BigDecimal)>(conn)?;

    let mut data_sources: Vec<StoredDynamicDataSource> = Vec::new();
    for (vid, name, context, address, creation_block) in dds.into_iter() {
        if address.len() != 20 {
            return Err(constraint_violation!(
                "Data source address `0x{:?}` for dynamic data source {} should be 20 bytes long but is {} bytes long",
                address, vid,
            address.len()
        ));
        }

        let manifest_idx = manifest_idx_and_name
            .iter()
            .find(|(_, manifest_name)| manifest_name == &name)
            .ok_or_else(|| constraint_violation!("data source name {} not found", name))?
            .0;
        let creation_block = creation_block.to_i32();
        let data_source = StoredDynamicDataSource {
            manifest_idx,
            param: Some(address.into()),
            context: context.map(|ctx| serde_json::from_str(&ctx)).transpose()?,
            creation_block,

            // The shared schema is only used for legacy deployments, and therefore not used for
            // subgraphs that use file data sources.
            done_at: None,
            causality_region: CausalityRegion::ONCHAIN,
        };

        if data_sources.last().and_then(|d| d.creation_block) > data_source.creation_block {
            return Err(StoreError::ConstraintViolation(
                "data sources not ordered by creation block".to_string(),
            ));
        }

        data_sources.push(data_source);
    }
    Ok(data_sources)
}

pub(super) fn insert(
    conn: &PgConnection,
    deployment: &DeploymentHash,
    data_sources: &write::DataSources,
    manifest_idx_and_name: &[(u32, String)],
) -> Result<usize, StoreError> {
    use dynamic_ethereum_contract_data_source as decds;

    if data_sources.is_empty() {
        // Avoids a roundtrip to the DB.
        return Ok(0);
    }

    let dds: Vec<_> = data_sources
        .entries
        .iter()
        .map(|(block_ptr, dds)| {
            dds.iter().map(|ds| {
                let StoredDynamicDataSource {
                    manifest_idx: _,
                    param,
                    context,
                    creation_block: _,
                    done_at: _,
                    causality_region,
                } = ds;

                if causality_region != &CausalityRegion::ONCHAIN {
                    return Err(constraint_violation!(
                        "using shared data source schema with file data sources"
                    ));
                }

                let address = match param {
                    Some(param) => param,
                    None => {
                        return Err(constraint_violation!(
                            "dynamic data sources must have an addres",
                        ));
                    }
                };
                let name = manifest_idx_and_name
                    .iter()
                    .find(|(idx, _)| *idx == ds.manifest_idx)
                    .ok_or_else(|| {
                        constraint_violation!("manifest idx {} not found", ds.manifest_idx)
                    })?
                    .1
                    .clone();
                Ok((
                    decds::deployment.eq(deployment.as_str()),
                    decds::name.eq(name),
                    decds::context.eq(context
                        .as_ref()
                        .map(|ctx| serde_json::to_string(ctx).unwrap())),
                    decds::address.eq(&**address),
                    decds::abi.eq(""),
                    decds::start_block.eq(0),
                    decds::ethereum_block_number.eq(sql(&format!("{}::numeric", block_ptr.number))),
                    decds::ethereum_block_hash.eq(block_ptr.hash_slice()),
                ))
            })
        })
        .flatten()
        .collect::<Result<_, _>>()?;

    insert_into(decds::table)
        .values(dds)
        .execute(conn)
        .map_err(|e| e.into())
}

/// Copy the dynamic data sources for `src` to `dst`. All data sources that
/// were created up to and including `target_block` will be copied.
pub(crate) fn copy(
    conn: &PgConnection,
    src: &Site,
    dst: &Site,
    target_block: BlockNumber,
) -> Result<usize, StoreError> {
    use dynamic_ethereum_contract_data_source as decds;

    // Subgraphs that use private data sources have no data in the shared
    // table
    if src.schema_version.private_data_sources() {
        return Ok(0);
    }

    let src_nsp = ForeignServer::metadata_schema_in(&src.shard, &dst.shard);

    // Check whether there are any dynamic data sources for dst which
    // indicates we already did copy
    let count = decds::table
        .filter(decds::deployment.eq(dst.deployment.as_str()))
        .select(count(decds::vid))
        .get_result::<i64>(conn)?;
    if count > 0 {
        return Ok(count as usize);
    }

    let query = format!(
        "\
      insert into subgraphs.dynamic_ethereum_contract_data_source(name,
             address, abi, start_block, ethereum_block_hash,
             ethereum_block_number, deployment, context)
      select e.name, e.address, e.abi, e.start_block,
             e.ethereum_block_hash, e.ethereum_block_number, $2 as deployment,
             e.context
        from {src_nsp}.dynamic_ethereum_contract_data_source e
       where e.deployment = $1
         and e.ethereum_block_number <= $3",
        src_nsp = src_nsp
    );

    Ok(sql_query(query)
        .bind::<Text, _>(src.deployment.as_str())
        .bind::<Text, _>(dst.deployment.as_str())
        .bind::<Integer, _>(target_block)
        .execute(conn)?)
}

pub(super) fn revert(
    conn: &PgConnection,
    id: &DeploymentHash,
    block: BlockNumber,
) -> Result<(), StoreError> {
    use dynamic_ethereum_contract_data_source as decds;

    let dds = decds::table.filter(decds::deployment.eq(id.as_str()));
    delete(dds.filter(decds::ethereum_block_number.ge(sql(&block.to_string())))).execute(conn)?;
    Ok(())
}

pub(crate) fn drop(conn: &PgConnection, id: &DeploymentHash) -> Result<usize, StoreError> {
    use dynamic_ethereum_contract_data_source as decds;

    delete(decds::table.filter(decds::deployment.eq(id.as_str())))
        .execute(conn)
        .map_err(|e| e.into())
}
