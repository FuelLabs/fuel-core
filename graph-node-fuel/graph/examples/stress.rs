use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::{BTreeMap, HashMap};
use std::iter::FromIterator;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;
use graph::data::value::{Object, Word};
use graph::object;
use graph::prelude::{lazy_static, q, r, BigDecimal, BigInt, QueryResult};
use rand::SeedableRng;
use rand::{rngs::SmallRng, Rng};

use graph::util::cache_weight::CacheWeight;
use graph::util::lfu_cache::LfuCache;

// Use a custom allocator that tracks how much memory the program
// has allocated overall

struct Counter;

static ALLOCATED: AtomicUsize = AtomicUsize::new(0);

lazy_static! {
    // Set 'MAP_MEASURE' to something to use the `CacheWeight` defined here
    // in the `btree` module for `BTreeMap`. If this is not set, use the
    // estimate from `graph::util::cache_weight`
    static ref MAP_MEASURE: bool = std::env::var("MAP_MEASURE").ok().is_some();

    // When running the `valuemap` test for BTreeMap, put maps into the
    // values of the generated maps
    static ref NESTED_MAP: bool =  std::env::var("NESTED_MAP").ok().is_some();
}
// Yes, a global variable. It gets set at the beginning of `main`
static mut PRINT_SAMPLES: bool = false;

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

mod btree {
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

    pub fn node_size<K, V>(map: &std::collections::BTreeMap<K, V>) -> usize {
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

        let sz = leaves * ln_sz + int_nodes * in_sz;

        if unsafe { super::PRINT_SAMPLES } {
            println!(
                " btree: leaves={} internal={} sz={} ln_sz={} in_sz={} len={}",
                leaves,
                int_nodes,
                sz,
                ln_sz,
                in_sz,
                map.len()
            );
        }
        sz
    }
}

struct MapMeasure<K, V>(BTreeMap<K, V>);

impl<K, V> Default for MapMeasure<K, V> {
    fn default() -> MapMeasure<K, V> {
        MapMeasure(BTreeMap::new())
    }
}

impl<K: CacheWeight, V: CacheWeight> CacheWeight for MapMeasure<K, V> {
    fn indirect_weight(&self) -> usize {
        if *MAP_MEASURE {
            let kv_sz = self
                .0
                .iter()
                .map(|(key, value)| key.indirect_weight() + value.indirect_weight())
                .sum::<usize>();
            let node_sz = btree::node_size(&self.0);
            kv_sz + node_sz
        } else {
            self.0.indirect_weight()
        }
    }
}

unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ret = System.alloc(layout);
        if !ret.is_null() {
            ALLOCATED.fetch_add(layout.size(), SeqCst);
        }
        ret
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
        ALLOCATED.fetch_sub(layout.size(), SeqCst);
    }
}

#[global_allocator]
static A: Counter = Counter;

// Setup to make checking different data types and how they interact
// with cache size easier

/// The template of an object we want to cache
trait Template: CacheWeight {
    // Create a new test object
    fn create(size: usize, rng: Option<&mut SmallRng>) -> Self;

    // Return a sample of this test object of the given `size`. There's no
    // fixed definition of 'size', other than that smaller sizes will
    // take less memory than larger ones
    fn sample(&self, size: usize, rng: Option<&mut SmallRng>) -> Box<Self>;
}

/// Template for testing caching of `String`
impl Template for String {
    fn create(size: usize, _rng: Option<&mut SmallRng>) -> Self {
        let mut s = String::with_capacity(size);
        for _ in 0..size {
            s.push('x');
        }
        s
    }
    fn sample(&self, size: usize, _rng: Option<&mut SmallRng>) -> Box<Self> {
        Box::new(self[0..size].into())
    }
}

/// Template for testing caching of `String`
impl Template for Word {
    fn create(size: usize, _rng: Option<&mut SmallRng>) -> Self {
        let mut s = String::with_capacity(size);
        for _ in 0..size {
            s.push('x');
        }
        Word::from(s)
    }

    fn sample(&self, size: usize, _rng: Option<&mut SmallRng>) -> Box<Self> {
        Box::new(self[0..size].into())
    }
}

/// Template for testing caching of `Vec<usize>`
impl Template for Vec<usize> {
    fn create(size: usize, _rng: Option<&mut SmallRng>) -> Self {
        Vec::from_iter(0..size)
    }
    fn sample(&self, size: usize, _rng: Option<&mut SmallRng>) -> Box<Self> {
        Box::new(self[0..size].into())
    }
}

impl Template for BigInt {
    fn create(size: usize, rng: Option<&mut SmallRng>) -> Self {
        let f = match rng {
            Some(rng) => {
                let mag = rng.gen_range(1..100);
                if rng.gen_bool(0.5) {
                    mag
                } else {
                    -mag
                }
            }
            None => 1,
        };
        BigInt::from(3u64).pow(size as u8).unwrap() * BigInt::from(f)
    }

    fn sample(&self, size: usize, rng: Option<&mut SmallRng>) -> Box<Self> {
        Box::new(Self::create(size, rng))
    }
}

impl Template for BigDecimal {
    fn create(size: usize, mut rng: Option<&mut SmallRng>) -> Self {
        let f = match rng.as_deref_mut() {
            Some(rng) => {
                let mag = rng.gen_range(1i32..100);
                if rng.gen_bool(0.5) {
                    mag
                } else {
                    -mag
                }
            }
            None => 1,
        };
        let exp = match rng {
            Some(rng) => rng.gen_range(-100..=100),
            None => 1,
        };
        let bi = BigInt::from(3u64).pow(size as u8).unwrap() * BigInt::from(f);
        BigDecimal::new(bi, exp)
    }

    fn sample(&self, size: usize, rng: Option<&mut SmallRng>) -> Box<Self> {
        Box::new(Self::create(size, rng))
    }
}

/// Template for testing caching of `HashMap<String, String>`
impl Template for HashMap<String, String> {
    fn create(size: usize, _rng: Option<&mut SmallRng>) -> Self {
        let mut map = HashMap::new();
        for i in 0..size {
            map.insert(format!("key{}", i), format!("value{}", i));
        }
        map
    }

    fn sample(&self, size: usize, _rng: Option<&mut SmallRng>) -> Box<Self> {
        Box::new(HashMap::from_iter(
            self.iter().take(size).map(|(k, v)| (k.clone(), v.clone())),
        ))
    }
}

fn make_object(size: usize, mut rng: Option<&mut SmallRng>) -> Object {
    let mut obj = Vec::new();
    let modulus = if *NESTED_MAP { 8 } else { 7 };

    for i in 0..size {
        let kind = rng
            .as_deref_mut()
            .map(|rng| rng.gen_range(0..modulus))
            .unwrap_or(i % modulus);

        let value = match kind {
            0 => r::Value::Boolean(i % 11 > 5),
            1 => r::Value::Int((i as i32).into()),
            2 => r::Value::Null,
            3 => r::Value::Float(i as f64 / 17.0),
            4 => r::Value::Enum(format!("enum{}", i)),
            5 => r::Value::String(format!("0x0000000000000000000000000000000000000000{}", i)),
            6 => {
                let vals = (0..(i % 51)).map(|i| r::Value::String(format!("list{}", i)));
                r::Value::List(Vec::from_iter(vals))
            }
            7 => {
                let mut obj = Vec::new();
                for j in 0..(i % 51) {
                    obj.push((
                        Word::from(format!("key{}", j)),
                        r::Value::String(format!("value{}", j)),
                    ));
                }
                r::Value::Object(Object::from_iter(obj))
            }
            _ => unreachable!(),
        };

        let key = rng.as_deref_mut().map(|rng| rng.gen()).unwrap_or(i) % modulus;
        obj.push((Word::from(format!("val{}", key)), value));
    }
    Object::from_iter(obj)
}

fn make_domains(size: usize, _rng: Option<&mut SmallRng>) -> Object {
    let owner = object! {
        owner: object! {
            id: "0xe8d391ef649a6652b9047735f6c0d48b6ae751df",
            name: "36190.eth"
        }
    };

    let domains: Vec<_> = (0..size).map(|_| owner.clone()).collect();
    Object::from_iter([("domains".into(), r::Value::List(domains))])
}

/// Template for testing caching of `Object`
impl Template for Object {
    fn create(size: usize, rng: Option<&mut SmallRng>) -> Self {
        make_object(size, rng)
    }

    fn sample(&self, size: usize, rng: Option<&mut SmallRng>) -> Box<Self> {
        // If the user specified '--fixed', don't build a new map every call
        // since that can be slow
        if rng.is_none() {
            Box::new(Object::from_iter(
                self.iter()
                    .take(size)
                    .map(|(k, v)| (Word::from(k), v.clone())),
            ))
        } else {
            Box::new(make_object(size, rng))
        }
    }
}

/// Template for testing caching of `QueryResult`
impl Template for QueryResult {
    fn create(size: usize, rng: Option<&mut SmallRng>) -> Self {
        QueryResult::new(make_domains(size, rng))
    }

    fn sample(&self, size: usize, rng: Option<&mut SmallRng>) -> Box<Self> {
        // If the user specified '--fixed', don't build a new map every call
        // since that can be slow
        if rng.is_none() {
            Box::new(QueryResult::new(Object::from_iter(
                self.data()
                    .unwrap()
                    .iter()
                    .take(size)
                    .map(|(k, v)| (Word::from(k), v.clone())),
            )))
        } else {
            Box::new(QueryResult::new(make_domains(size, rng)))
        }
    }
}

type ValueMap = MapMeasure<String, q::Value>;

impl ValueMap {
    fn make_map(size: usize, mut rng: Option<&mut SmallRng>) -> Self {
        let mut map = BTreeMap::new();
        let modulus = if *NESTED_MAP { 9 } else { 8 };

        for i in 0..size {
            let kind = rng
                .as_deref_mut()
                .map(|rng| rng.gen_range(0..modulus))
                .unwrap_or(i % modulus);

            let value = match kind {
                0 => q::Value::Boolean(i % 11 > 5),
                1 => q::Value::Int((i as i32).into()),
                2 => q::Value::Null,
                3 => q::Value::Float(i as f64 / 17.0),
                4 => q::Value::Enum(format!("enum{}", i)),
                5 => q::Value::String(format!("string{}", i)),
                6 => q::Value::Variable(format!("var{}", i)),
                7 => {
                    let vals = (0..(i % 51)).map(|i| q::Value::String(format!("list{}", i)));
                    q::Value::List(Vec::from_iter(vals))
                }
                8 => {
                    let mut map = BTreeMap::new();
                    for j in 0..(i % 51) {
                        map.insert(format!("key{}", j), q::Value::String(format!("value{}", j)));
                    }
                    q::Value::Object(map)
                }
                _ => unreachable!(),
            };

            let key = rng.as_deref_mut().map(|rng| rng.gen()).unwrap_or(i) % modulus;
            map.insert(format!("val{}", key), value);
        }
        MapMeasure(map)
    }
}

/// Template for testing roughly a GraphQL response, i.e., a `BTreeMap<String, Value>`
impl Template for ValueMap {
    fn create(size: usize, rng: Option<&mut SmallRng>) -> Self {
        Self::make_map(size, rng)
    }

    fn sample(&self, size: usize, rng: Option<&mut SmallRng>) -> Box<Self> {
        // If the user specified '--fixed', don't build a new map every call
        // since that can be slow
        if rng.is_none() {
            Box::new(MapMeasure(BTreeMap::from_iter(
                self.0
                    .iter()
                    .take(size)
                    .map(|(k, v)| (k.clone(), v.clone())),
            )))
        } else {
            Box::new(Self::make_map(size, rng))
        }
    }
}

type UsizeMap = MapMeasure<usize, usize>;

impl UsizeMap {
    fn make_map(size: usize, mut rng: Option<&mut SmallRng>) -> Self {
        let mut map = BTreeMap::new();
        for i in 0..size {
            let key = rng.as_deref_mut().map(|rng| rng.gen()).unwrap_or(2 * i);
            map.insert(key, i * 3);
        }
        MapMeasure(map)
    }
}

/// Template for testing roughly a GraphQL response, i.e., a `BTreeMap<String, Value>`
impl Template for UsizeMap {
    fn create(size: usize, rng: Option<&mut SmallRng>) -> Self {
        Self::make_map(size, rng)
    }

    fn sample(&self, size: usize, rng: Option<&mut SmallRng>) -> Box<Self> {
        // If the user specified '--fixed', don't build a new map every call
        // since that can be slow
        if rng.is_none() {
            Box::new(MapMeasure(BTreeMap::from_iter(
                self.0
                    .iter()
                    .take(size)
                    .map(|(k, v)| (k.to_owned(), v.to_owned())),
            )))
        } else {
            Box::new(Self::make_map(size, rng))
        }
    }
}

/// Wrapper around our template objects; we always put them behind an `Arc`
/// so that dropping the template object frees the entire object rather than
/// leaving part of it in the cache. That's also how the production code
/// uses this cache: objects always wrapped in an `Arc`
struct Entry<T>(Arc<T>);

impl<T: Template> Default for Entry<T> {
    fn default() -> Self {
        Self(Arc::new(T::create(0, None)))
    }
}

impl<T: Template> CacheWeight for Entry<T> {
    fn indirect_weight(&self) -> usize {
        // Account for the two pointers the Arc uses for keeping reference
        // counts. Including that in the weight is only correct in this
        // test, since we know we never have more than one reference throug
        // the `Arc`
        self.0.weight() + 2 * std::mem::size_of::<usize>()
    }
}

impl<T: Template> From<T> for Entry<T> {
    fn from(templ: T) -> Self {
        Self(Arc::new(templ))
    }
}

// Command line arguments
#[derive(Parser)]
#[clap(name = "stress", about = "Stress test for the LFU Cache")]
struct Opt {
    /// Number of cache evictions and insertions
    #[clap(short, long, default_value = "1000")]
    niter: usize,
    /// Print this many intermediate messages
    #[clap(short, long, default_value = "10")]
    print_count: usize,
    /// Use objects of size 0 up to this size, chosen unifromly randomly
    /// unless `--fixed` is given
    #[clap(short, long, default_value = "1024")]
    obj_size: usize,
    #[clap(short, long, default_value = "1000000")]
    cache_size: usize,
    #[clap(short, long, default_value = "vec")]
    template: String,
    #[clap(short, long)]
    samples: bool,
    /// Always use objects of size `--obj-size`
    #[clap(short, long)]
    fixed: bool,
    /// The seed of the random number generator. A seed of 0 means that all
    /// samples are taken from the same template object, and only differ in
    /// size
    #[clap(long)]
    seed: Option<u64>,
}

fn maybe_rng<'a>(opt: &'a Opt, rng: &'a mut SmallRng) -> Option<&'a mut SmallRng> {
    if opt.seed == Some(0) {
        None
    } else {
        Some(rng)
    }
}

fn stress<T: Template>(opt: &Opt) {
    let mut rng = match opt.seed {
        None => SmallRng::from_entropy(),
        Some(seed) => SmallRng::seed_from_u64(seed),
    };

    let mut cache: LfuCache<usize, Entry<T>> = LfuCache::new();
    let template = T::create(opt.obj_size, maybe_rng(opt, &mut rng));

    println!("type: {}", std::any::type_name::<T>());
    println!(
        "obj weight: {} iterations: {} cache_size: {}\n",
        template.weight(),
        opt.niter,
        opt.cache_size
    );

    let base_mem = ALLOCATED.load(SeqCst);
    let print_mod = opt.niter / opt.print_count + 1;
    let mut should_print = true;
    let mut print_header = true;
    let mut sample_weight: usize = 0;
    let mut sample_alloc: usize = 0;
    let mut evict_count: usize = 0;
    let mut evict_duration = Duration::from_secs(0);

    let start = Instant::now();
    for key in 0..opt.niter {
        should_print = should_print || key % print_mod == 0;
        let before_mem = ALLOCATED.load(SeqCst);
        let start_evict = Instant::now();
        if let Some(stats) = cache.evict(opt.cache_size) {
            evict_duration += start_evict.elapsed();
            let after_mem = ALLOCATED.load(SeqCst);
            evict_count += 1;
            if should_print {
                if print_header {
                    println!("evict:        weight that was removed from cache");
                    println!("drop:         allocated memory that was freed");
                    println!("slip:         evicted - dropped");
                    println!("room:         configured cache size - cache weight");
                    println!("heap:         allocated heap_size / configured cache_size");
                    println!("mem:          memory allocated for cache + all entries\n");
                    print_header = false;
                }

                let dropped = before_mem - after_mem;
                let evicted_count = stats.evicted_count;
                let evicted = stats.evicted_weight;
                let slip = evicted as i64 - dropped as i64;
                let room = opt.cache_size as i64 - stats.new_weight as i64;
                let heap = (after_mem - base_mem) as f64 / opt.cache_size as f64;
                let mem = after_mem - base_mem;
                println!(
                    "evict: [{evicted_count:3}]{evicted:6}  drop: {dropped:6} slip: {slip:4} \
                    room: {room:6} heap: {heap:0.2}  mem: {mem:8}"
                );
                should_print = false;
            }
        }
        let size = if opt.fixed || opt.obj_size == 0 {
            opt.obj_size
        } else {
            rng.gen_range(0..opt.obj_size)
        };
        let before = ALLOCATED.load(SeqCst);
        let sample = template.sample(size, maybe_rng(opt, &mut rng));
        let weight = sample.weight();
        let alloc = ALLOCATED.load(SeqCst) - before;
        sample_weight += weight;
        sample_alloc += alloc;
        if opt.samples {
            println!("sample: weight {:6} alloc {:6}", weight, alloc,);
        }
        cache.insert(key, Entry::from(*sample));
        // Do a few random reads from the cache
        for _attempt in 0..5 {
            let read = rng.gen_range(0..=key);
            let _v = cache.get(&read);
        }
    }

    println!(
        "\ncache: entries: {} evictions: {} took {}ms out of {}ms",
        cache.len(),
        evict_count,
        evict_duration.as_millis(),
        start.elapsed().as_millis()
    );
    if sample_alloc == sample_weight {
        println!(
            "samples: weight {} alloc {} weight/alloc precise",
            sample_weight, sample_alloc
        );
    } else {
        let heap_factor = sample_alloc as f64 / sample_weight as f64;
        println!(
            "samples: weight {} alloc {} weight/alloc {:0.2}",
            sample_weight, sample_alloc, heap_factor
        );
    }
}

/// This program constructs a template object of size `obj_size` and then
/// inserts a sample of size up to `obj_size` into the cache `niter` times.
/// The cache is limited to `cache_size` total weight, and we call `evict`
/// before each insertion into the cache.
///
/// After each `evict`, we check how much heap we have currently allocated
/// and print that roughly `print_count` times over the run of the program.
/// The most important measure is the `heap_factor`, which is the ratio of
/// memory used on the heap since we started inserting into the cache to
/// the target `cache_size`
pub fn main() {
    let opt = Opt::from_args();
    unsafe { PRINT_SAMPLES = opt.samples }

    // Use different Cacheables to see how the cache manages memory with
    // different types of cache entries.
    match opt.template.as_str() {
        "bigdecimal" => stress::<BigDecimal>(&opt),
        "bigint" => stress::<BigInt>(&opt),
        "hashmap" => stress::<HashMap<String, String>>(&opt),
        "object" => stress::<Object>(&opt),
        "result" => stress::<QueryResult>(&opt),
        "string" => stress::<String>(&opt),
        "usizemap" => stress::<UsizeMap>(&opt),
        "valuemap" => stress::<ValueMap>(&opt),
        "vec" => stress::<Vec<usize>>(&opt),
        "word" => stress::<Word>(&opt),
        _ => println!("unknown value `{}` for --template", opt.template),
    }
}
