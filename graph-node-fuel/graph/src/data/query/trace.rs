use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use serde::{ser::SerializeMap, Serialize};

use crate::{components::store::BlockNumber, prelude::CheapClone};

#[derive(Debug)]
pub enum Trace {
    None,
    Root {
        query: Arc<String>,
        variables: Arc<String>,
        query_id: String,
        block: BlockNumber,
        elapsed: Mutex<Duration>,
        conn_wait: Duration,
        permit_wait: Duration,
        children: Vec<(String, Trace)>,
    },
    Query {
        query: String,
        elapsed: Duration,
        conn_wait: Duration,
        permit_wait: Duration,
        entity_count: usize,
        children: Vec<(String, Trace)>,
    },
}

impl Default for Trace {
    fn default() -> Self {
        Self::None
    }
}

impl Trace {
    pub fn root(
        query: &Arc<String>,
        variables: &Arc<String>,
        query_id: &str,
        block: BlockNumber,
        do_trace: bool,
    ) -> Trace {
        if do_trace {
            Trace::Root {
                query: query.cheap_clone(),
                variables: variables.cheap_clone(),
                query_id: query_id.to_string(),
                block,
                elapsed: Mutex::new(Duration::from_millis(0)),
                conn_wait: Duration::from_millis(0),
                permit_wait: Duration::from_millis(0),
                children: Vec::new(),
            }
        } else {
            Trace::None
        }
    }

    pub fn finish(&self, dur: Duration) {
        match self {
            Trace::None | Trace::Query { .. } => { /* nothing to do */ }
            Trace::Root { elapsed, .. } => *elapsed.lock().unwrap() = dur,
        }
    }

    pub fn query(query: &str, elapsed: Duration, entity_count: usize) -> Trace {
        Trace::Query {
            query: query.to_string(),
            elapsed,
            conn_wait: Duration::from_millis(0),
            permit_wait: Duration::from_millis(0),
            entity_count,
            children: Vec::new(),
        }
    }

    pub fn push(&mut self, name: &str, trace: Trace) {
        match (self, &trace) {
            (Self::Root { children, .. }, Self::Query { .. }) => {
                children.push((name.to_string(), trace))
            }
            (Self::Query { children, .. }, Self::Query { .. }) => {
                children.push((name.to_string(), trace))
            }
            (Self::None, Self::None) | (Self::Root { .. }, Self::None) => { /* tracing is turned off */
            }
            (s, t) => {
                unreachable!("can not add child self: {:#?} trace: {:#?}", s, t)
            }
        }
    }

    pub fn is_none(&self) -> bool {
        match self {
            Trace::None => true,
            Trace::Root { .. } | Trace::Query { .. } => false,
        }
    }

    pub fn conn_wait(&mut self, time: Duration) {
        match self {
            Trace::None => { /* nothing to do  */ }
            Trace::Root { conn_wait, .. } | Trace::Query { conn_wait, .. } => *conn_wait += time,
        }
    }

    pub fn permit_wait(&mut self, time: Duration) {
        match self {
            Trace::None => { /* nothing to do  */ }
            Trace::Root { permit_wait, .. } | Trace::Query { permit_wait, .. } => {
                *permit_wait += time
            }
        }
    }
}

impl Serialize for Trace {
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Trace::None => ser.serialize_none(),
            Trace::Root {
                query,
                variables,
                query_id,
                block,
                elapsed,
                conn_wait,
                permit_wait,
                children,
            } => {
                let mut map = ser.serialize_map(Some(children.len() + 2))?;
                map.serialize_entry("query", query)?;
                if !variables.is_empty() && variables.as_str() != "{}" {
                    map.serialize_entry("variables", variables)?;
                }
                map.serialize_entry("query_id", query_id)?;
                map.serialize_entry("block", block)?;
                map.serialize_entry("elapsed_ms", &elapsed.lock().unwrap().as_millis())?;
                for (child, trace) in children {
                    map.serialize_entry(child, trace)?;
                }
                map.serialize_entry("conn_wait_ms", &conn_wait.as_millis())?;
                map.serialize_entry("permit_wait_ms", &permit_wait.as_millis())?;
                map.end()
            }
            Trace::Query {
                query,
                elapsed,
                conn_wait,
                permit_wait,
                entity_count,
                children,
            } => {
                let mut map = ser.serialize_map(Some(children.len() + 3))?;
                map.serialize_entry("query", query)?;
                map.serialize_entry("elapsed_ms", &elapsed.as_millis())?;
                map.serialize_entry("conn_wait_ms", &conn_wait.as_millis())?;
                map.serialize_entry("permit_wait_ms", &permit_wait.as_millis())?;
                map.serialize_entry("entity_count", entity_count)?;
                for (child, trace) in children {
                    map.serialize_entry(child, trace)?;
                }
                map.end()
            }
        }
    }
}
