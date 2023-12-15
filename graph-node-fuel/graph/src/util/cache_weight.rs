use crate::{
    data::value::Word,
    prelude::{q, BigDecimal, BigInt, Value},
    schema::{EntityKey, EntityType},
};
use std::{
    collections::{BTreeMap, HashMap},
    mem,
};

/// Estimate of how much memory a value consumes.
/// Useful for measuring the size of caches.
pub trait CacheWeight {
    /// Total weight of the value.
    fn weight(&self) -> usize {
        mem::size_of_val(self) + self.indirect_weight()
    }

    /// The weight of values pointed to by this value but logically owned by it, which is not
    /// accounted for by `size_of`.
    fn indirect_weight(&self) -> usize;
}

impl<T: CacheWeight> CacheWeight for Option<T> {
    fn indirect_weight(&self) -> usize {
        match self {
            Some(x) => x.indirect_weight(),
            None => 0,
        }
    }
}

impl<T: CacheWeight> CacheWeight for Vec<T> {
    fn indirect_weight(&self) -> usize {
        self.iter().map(CacheWeight::indirect_weight).sum::<usize>()
            + self.capacity() * mem::size_of::<T>()
    }
}

impl<T: CacheWeight, U: CacheWeight> CacheWeight for BTreeMap<T, U> {
    fn indirect_weight(&self) -> usize {
        self.iter()
            .map(|(key, value)| key.indirect_weight() + value.indirect_weight())
            .sum::<usize>()
            + btree::node_size(self)
    }
}

impl<T: CacheWeight, U: CacheWeight> CacheWeight for HashMap<T, U> {
    fn indirect_weight(&self) -> usize {
        self.iter()
            .map(|(key, value)| key.indirect_weight() + value.indirect_weight())
            .sum::<usize>()
            + self.capacity() * mem::size_of::<(T, U, u64)>()
    }
}

impl CacheWeight for String {
    fn indirect_weight(&self) -> usize {
        self.capacity()
    }
}

impl CacheWeight for Word {
    fn indirect_weight(&self) -> usize {
        self.len()
    }
}

impl CacheWeight for BigDecimal {
    fn indirect_weight(&self) -> usize {
        ((self.digits() as f32 * std::f32::consts::LOG2_10) / 8.0).ceil() as usize
    }
}

impl CacheWeight for BigInt {
    fn indirect_weight(&self) -> usize {
        self.bits() / 8
    }
}

impl CacheWeight for crate::data::store::scalar::Bytes {
    fn indirect_weight(&self) -> usize {
        self.as_slice().len()
    }
}

impl CacheWeight for Value {
    fn indirect_weight(&self) -> usize {
        match self {
            Value::String(s) => s.indirect_weight(),
            Value::BigDecimal(d) => d.indirect_weight(),
            Value::List(values) => values.indirect_weight(),
            Value::Bytes(bytes) => bytes.indirect_weight(),
            Value::BigInt(n) => n.indirect_weight(),
            Value::Int8(_) | Value::Int(_) | Value::Bool(_) | Value::Null => 0,
        }
    }
}

impl CacheWeight for q::Value {
    fn indirect_weight(&self) -> usize {
        match self {
            q::Value::Boolean(_) | q::Value::Int(_) | q::Value::Null | q::Value::Float(_) => 0,
            q::Value::Enum(s) | q::Value::String(s) | q::Value::Variable(s) => s.indirect_weight(),
            q::Value::List(l) => l.indirect_weight(),
            q::Value::Object(o) => o.indirect_weight(),
        }
    }
}

impl CacheWeight for usize {
    fn indirect_weight(&self) -> usize {
        0
    }
}

impl CacheWeight for EntityType {
    fn indirect_weight(&self) -> usize {
        0
    }
}

impl CacheWeight for EntityKey {
    fn indirect_weight(&self) -> usize {
        self.entity_id.indirect_weight() + self.entity_type.indirect_weight()
    }
}

impl CacheWeight for [u8; 32] {
    fn indirect_weight(&self) -> usize {
        0
    }
}

#[cfg(test)]
impl CacheWeight for &'static str {
    fn indirect_weight(&self) -> usize {
        0
    }
}

#[test]
fn big_decimal_cache_weight() {
    use std::str::FromStr;

    // 22.4548 has 18 bits as binary, so 3 bytes.
    let n = BigDecimal::from_str("22.454800000000").unwrap();
    assert_eq!(n.indirect_weight(), 3);
}

/// Helpers to estimate the size of a `BTreeMap`. Everything in this module,
/// except for `node_size()` is copied from `std::collections::btree`.
///
/// It is not possible to know how many nodes a BTree has, as
/// `BTreeMap` does not expose its depth or any other detail about
/// the true size of the BTree. We estimate that size, assuming the
/// average case, i.e., a BTree where every node has the average
/// between the minimum and maximum number of entries per node, i.e.,
/// the average of (B-1) and (2*B-1) entries, which we call
/// `NODE_FILL`. The number of leaf nodes in the tree is then the
/// number of entries divided by `NODE_FILL`, and the number of
/// interior nodes can be determined by dividing the number of nodes
/// at the child level by `NODE_FILL`

/// The other difficulty is that the structs with which `BTreeMap`
/// represents internal and leaf nodes are not public, so we can't
/// get their size with `std::mem::size_of`; instead, we base our
/// estimates of their size on the current `std` code, assuming that
/// these structs will not change

pub mod btree {
    use std::collections::BTreeMap;
    use std::mem;
    use std::{mem::MaybeUninit, ptr::NonNull};

    const B: usize = 6;
    const CAPACITY: usize = 2 * B - 1;

    /// Assume BTree nodes are this full (average of minimum and maximum fill)
    const NODE_FILL: usize = ((B - 1) + (2 * B - 1)) / 2;

    type BoxedNode<K, V> = NonNull<LeafNode<K, V>>;

    struct InternalNode<K, V> {
        _data: LeafNode<K, V>,

        /// The pointers to the children of this node. `len + 1` of these are considered
        /// initialized and valid, except that near the end, while the tree is held
        /// through borrow type `Dying`, some of these pointers are dangling.
        _edges: [MaybeUninit<BoxedNode<K, V>>; 2 * B],
    }

    struct LeafNode<K, V> {
        /// We want to be covariant in `K` and `V`.
        _parent: Option<NonNull<InternalNode<K, V>>>,

        /// This node's index into the parent node's `edges` array.
        /// `*node.parent.edges[node.parent_idx]` should be the same thing as `node`.
        /// This is only guaranteed to be initialized when `parent` is non-null.
        _parent_idx: MaybeUninit<u16>,

        /// The number of keys and values this node stores.
        _len: u16,

        /// The arrays storing the actual data of the node. Only the first `len` elements of each
        /// array are initialized and valid.
        _keys: [MaybeUninit<K>; CAPACITY],
        _vals: [MaybeUninit<V>; CAPACITY],
    }

    /// Estimate the size of the BTreeMap `map` ignoring the size of any keys
    /// and values
    pub fn node_size<K, V>(map: &BTreeMap<K, V>) -> usize {
        // Measure the size of internal and leaf nodes directly - that's why
        // we copied all this code from `std`
        let ln_sz = mem::size_of::<LeafNode<K, V>>();
        let in_sz = mem::size_of::<InternalNode<K, V>>();

        // Estimate the number of internal and leaf nodes based on the only
        // thing we can measure about a BTreeMap, the number of entries in
        // it, and use our `NODE_FILL` assumption to estimate how the tree
        // is structured. We try to be very good for small maps, since
        // that's what we use most often in our code. This estimate is only
        // for the indirect weight of the `BTreeMap`
        let (leaves, int_nodes) = if map.is_empty() {
            // An empty tree has no indirect weight
            (0, 0)
        } else if map.len() <= CAPACITY {
            // We only have the root node
            (1, 0)
        } else {
            // Estimate based on our `NODE_FILL` assumption
            let leaves = map.len() / NODE_FILL + 1;
            let mut prev_level = leaves / NODE_FILL + 1;
            let mut int_nodes = prev_level;
            while prev_level > 1 {
                int_nodes += prev_level;
                prev_level = prev_level / NODE_FILL + 1;
            }
            (leaves, int_nodes)
        };

        leaves * ln_sz + int_nodes * in_sz
    }
}
