use std::iter::FromIterator;
use std::sync::Arc;
use std::{collections::BTreeSet, io::Write};

use futures::compat::Future01CompatExt;
use graph::prelude::DeploymentHash;
use graph::schema::{EntityType, InputSchema};
use graph::{
    components::store::SubscriptionManager as _,
    prelude::{serde_json, Error, Stream, SubscriptionFilter},
};
use graph_store_postgres::connection_pool::ConnectionPool;
use graph_store_postgres::SubscriptionManager;

use crate::manager::deployment::DeploymentSearch;

async fn listen(
    mgr: Arc<SubscriptionManager>,
    filter: BTreeSet<SubscriptionFilter>,
) -> Result<(), Error> {
    let events = mgr.subscribe(filter);
    println!("press ctrl-c to stop");
    let res = events
        .inspect(move |event| {
            serde_json::to_writer_pretty(std::io::stdout(), event)
                .expect("event can be serialized to JSON");
            writeln!(std::io::stdout()).unwrap();
            std::io::stdout().flush().unwrap();
        })
        .collect()
        .compat()
        .await;

    match res {
        Ok(_) => {
            println!("stream finished")
        }
        Err(()) => {
            eprintln!("stream failed")
        }
    }
    Ok(())
}

pub async fn assignments(mgr: Arc<SubscriptionManager>) -> Result<(), Error> {
    println!("waiting for assignment events");
    listen(
        mgr,
        FromIterator::from_iter([SubscriptionFilter::Assignment]),
    )
    .await?;

    Ok(())
}

pub async fn entities(
    primary_pool: ConnectionPool,
    mgr: Arc<SubscriptionManager>,
    search: &DeploymentSearch,
    entity_types: Vec<String>,
) -> Result<(), Error> {
    // We convert the entity type names into entity types in this very
    // awkward way to avoid needing to have a SubgraphStore from which we
    // load the input schema
    fn as_entity_types(
        entity_types: Vec<String>,
        id: &DeploymentHash,
    ) -> Result<Vec<EntityType>, Error> {
        use std::fmt::Write;

        let schema = entity_types
            .iter()
            .fold(String::new(), |mut buf, entity_type| {
                writeln!(buf, "type {entity_type} @entity {{ id: ID! }}").unwrap();
                buf
            });
        let schema = InputSchema::parse(&schema, id.clone()).unwrap();
        entity_types
            .iter()
            .map(|et| schema.entity_type(et))
            .collect::<Result<_, _>>()
    }

    let locator = search.locate_unique(&primary_pool)?;
    let filter = as_entity_types(entity_types, &locator.hash)?
        .into_iter()
        .map(|et| SubscriptionFilter::Entities(locator.hash.clone(), et))
        .collect();

    println!("waiting for store events from {}", locator);
    listen(mgr, filter).await?;

    Ok(())
}
