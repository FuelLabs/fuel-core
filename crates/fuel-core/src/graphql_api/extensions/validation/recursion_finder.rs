use async_graphql::{
    Positioned,
    parser::types::Field,
};
use std::collections::{
    HashMap,
    hash_map::Entry,
};

use super::visitor::{
    Visitor,
    VisitorContext,
};

pub(super) struct RecursionFinder<'a> {
    visited: HashMap<&'a str, usize>,
    recursion_limit: usize,
}

impl RecursionFinder<'_> {
    pub fn new(recursion_limit: usize) -> Self {
        Self {
            visited: Default::default(),
            recursion_limit,
        }
    }
}

impl<'a> Visitor<'a> for RecursionFinder<'a> {
    fn enter_field(
        &mut self,
        ctx: &mut VisitorContext<'a>,
        field: &'a Positioned<Field>,
    ) {
        let ty = ctx.type_stack.last();

        if let Some(Some(ty)) = ty {
            let name = ty.name();

            if name == "__Type" {
                return
            }

            let old = self.visited.entry(name).or_default();
            *old = old.saturating_add(1);

            if *old > self.recursion_limit {
                ctx.report_error(
                    vec![field.pos],
                    format!("Recursion detected for field `{}`", field.node.name),
                );
            }
        }
    }

    fn exit_field(&mut self, ctx: &mut VisitorContext<'a>, _: &'a Positioned<Field>) {
        let ty = ctx.type_stack.last();

        if let Some(Some(ty)) = ty {
            let name = ty.name();

            if name == "__Type" {
                return
            }

            let old = self.visited.entry(name);
            match old {
                Entry::Occupied(entry) => {
                    if entry.get() == &1 {
                        entry.remove();
                    } else {
                        let value = entry.into_mut();
                        *value = value.saturating_sub(1);
                    }
                }
                Entry::Vacant(_) => {
                    // Shouldn't be possible.
                }
            }
        }
    }
}
