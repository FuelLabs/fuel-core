//! Calculate a hash for a GraphQL query that reflects the shape of
//! the query. The shape hash will be the same for two instances of a query
//! that are deemed identical except for unimportant details. Those details
//! are any values used with filters, and any differences in the query
//! name or response keys

use crate::prelude::{q, s};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

type ShapeHasher = DefaultHasher;

pub trait ShapeHash {
    fn shape_hash(&self, hasher: &mut ShapeHasher);
}

pub fn shape_hash(query: &q::Document) -> u64 {
    let mut hasher = DefaultHasher::new();
    query.shape_hash(&mut hasher);
    hasher.finish()
}

// In all ShapeHash implementations, we never include anything to do with
// the position of the element in the query, i.e., fields that involve
// `Pos`

impl ShapeHash for q::Document {
    fn shape_hash(&self, hasher: &mut ShapeHasher) {
        for defn in &self.definitions {
            use q::Definition::*;
            match defn {
                Operation(op) => op.shape_hash(hasher),
                Fragment(frag) => frag.shape_hash(hasher),
            }
        }
    }
}

impl ShapeHash for q::OperationDefinition {
    fn shape_hash(&self, hasher: &mut ShapeHasher) {
        use graphql_parser::query::OperationDefinition::*;
        // We want `[query|subscription|mutation] things { BODY }` to hash
        // to the same thing as just `things { BODY }`
        match self {
            SelectionSet(set) => set.shape_hash(hasher),
            Query(query) => query.selection_set.shape_hash(hasher),
            Mutation(mutation) => mutation.selection_set.shape_hash(hasher),
            Subscription(subscription) => subscription.selection_set.shape_hash(hasher),
        }
    }
}

impl ShapeHash for q::FragmentDefinition {
    fn shape_hash(&self, hasher: &mut ShapeHasher) {
        // Omit directives
        self.name.hash(hasher);
        self.type_condition.shape_hash(hasher);
        self.selection_set.shape_hash(hasher);
    }
}

impl ShapeHash for q::SelectionSet {
    fn shape_hash(&self, hasher: &mut ShapeHasher) {
        for item in &self.items {
            item.shape_hash(hasher);
        }
    }
}

impl ShapeHash for q::Selection {
    fn shape_hash(&self, hasher: &mut ShapeHasher) {
        use graphql_parser::query::Selection::*;
        match self {
            Field(field) => field.shape_hash(hasher),
            FragmentSpread(spread) => spread.shape_hash(hasher),
            InlineFragment(frag) => frag.shape_hash(hasher),
        }
    }
}

impl ShapeHash for q::Field {
    fn shape_hash(&self, hasher: &mut ShapeHasher) {
        // Omit alias, directives
        self.name.hash(hasher);
        self.selection_set.shape_hash(hasher);
        for (name, value) in &self.arguments {
            name.hash(hasher);
            value.shape_hash(hasher);
        }
    }
}

impl ShapeHash for s::Value {
    fn shape_hash(&self, hasher: &mut ShapeHasher) {
        use graphql_parser::schema::Value::*;

        match self {
            Variable(_) | Int(_) | Float(_) | String(_) | Boolean(_) | Null | Enum(_) => {
                /* ignore */
            }
            List(values) => {
                for value in values {
                    value.shape_hash(hasher);
                }
            }
            Object(map) => {
                for (name, value) in map {
                    name.hash(hasher);
                    value.shape_hash(hasher);
                }
            }
        }
    }
}

impl ShapeHash for q::FragmentSpread {
    fn shape_hash(&self, hasher: &mut ShapeHasher) {
        // Omit directives
        self.fragment_name.hash(hasher)
    }
}

impl ShapeHash for q::InlineFragment {
    fn shape_hash(&self, hasher: &mut ShapeHasher) {
        // Omit directives
        self.type_condition.shape_hash(hasher);
        self.selection_set.shape_hash(hasher);
    }
}

impl<T: ShapeHash> ShapeHash for Option<T> {
    fn shape_hash(&self, hasher: &mut ShapeHasher) {
        match self {
            None => false.hash(hasher),
            Some(t) => {
                Some(true).hash(hasher);
                t.shape_hash(hasher);
            }
        }
    }
}

impl ShapeHash for q::TypeCondition {
    fn shape_hash(&self, hasher: &mut ShapeHasher) {
        match self {
            q::TypeCondition::On(value) => value.hash(hasher),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphql_parser::parse_query;

    #[test]
    fn identical_and_different() {
        const Q1: &str = "query things($stuff: Int) { things(where: { stuff_gt: $stuff }) { id } }";
        const Q2: &str = "{ things(where: { stuff_gt: 42 }) { id } }";
        const Q3: &str = "{ things(where: { stuff_lte: 42 }) { id } }";
        const Q4: &str = "{ things(where: { stuff_gt: 42 }) { id name } }";
        let q1 = parse_query(Q1)
            .expect("q1 is syntactically valid")
            .into_static();
        let q2 = parse_query(Q2)
            .expect("q2 is syntactically valid")
            .into_static();
        let q3 = parse_query(Q3)
            .expect("q3 is syntactically valid")
            .into_static();
        let q4 = parse_query(Q4)
            .expect("q4 is syntactically valid")
            .into_static();

        assert_eq!(shape_hash(&q1), shape_hash(&q2));
        assert_ne!(shape_hash(&q1), shape_hash(&q3));
        assert_ne!(shape_hash(&q2), shape_hash(&q4));
    }
}
