use graph::data::subgraph::schema::DeploymentCreate;
use graph::entity;
use graph::prelude::{SubscriptionResult, Value};
use graph::schema::InputSchema;
use graphql_parser::Pos;
use std::iter::FromIterator;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{
    collections::{BTreeSet, HashMap},
    marker::PhantomData,
};

use graph::{
    components::store::DeploymentLocator,
    data::graphql::{object, object_value},
    data::subgraph::schema::SubgraphError,
    data::{
        query::{QueryResults, QueryTarget},
        subgraph::SubgraphFeature,
    },
    prelude::{
        futures03::stream::StreamExt, lazy_static, o, q, r, serde_json, slog, BlockPtr,
        DeploymentHash, Entity, EntityOperation, FutureExtension, GraphQlRunner as _, Logger,
        NodeId, Query, QueryError, QueryExecutionError, QueryResult, QueryStoreManager,
        QueryVariables, SubgraphManifest, SubgraphName, SubgraphStore,
        SubgraphVersionSwitchingMode, Subscription, SubscriptionError,
    },
    semver::Version,
};
use graph_graphql::{prelude::*, subscription::execute_subscription};
use test_store::{
    deployment_state, execute_subgraph_query, execute_subgraph_query_with_deadline,
    graphql_metrics, revert_block, run_test_sequentially, transact_errors, Store, BLOCK_ONE,
    GENESIS_PTR, LOAD_MANAGER, LOGGER, METRICS_REGISTRY, STORE, SUBSCRIPTION_MANAGER,
};

/// Ids for the various entities that we create in `insert_entities` and
/// access through `IdType` to check results in the tests
const NETWORK_NAME: &str = "fake_network";
const SONGS_STRING: [&str; 5] = ["s0", "s1", "s2", "s3", "s4"];
const SONGS_BYTES: [&str; 5] = ["0xf0", "0xf1", "0xf2", "0xf3", "0xf4"];
const SONGS_INT: [&str; 5] = ["42", "43", "44", "45", "46"];
const MEDIA_STRING: [&str; 7] = ["md0", "md1", "md2", "md3", "md4", "md5", "md6"];
const MEDIA_BYTES: [&str; 7] = ["0xf0", "0xf1", "0xf2", "0xf3", "0xf4", "0xf5", "0xf6"];
const MEDIA_INT: [&str; 7] = ["52", "53", "54", "55", "56", "57", "58"];

lazy_static! {
    /// The id of the sole publisher in the test data
    static ref PUB1: IdVal = IdType::Bytes.parse("0xb1");
}

/// A convenience wrapper for `Value` and `r::Value` that clones a lot,
/// which is fine in tests, in order to keep test notation concise
#[derive(Debug)]
struct IdVal(Value);

impl IdVal {
    fn as_gql(&self, id_type: IdType) -> String {
        match (id_type, self) {
            (IdType::String, IdVal(Value::String(s))) => format!("\"{}\"", s),
            (IdType::Bytes, IdVal(Value::Bytes(b))) => format!("\"{}\"", b),
            (IdType::Int8, IdVal(Value::Int8(i))) => format!("{}", i),
            _ => panic!(
                "Invalid combination of id type {} and value {self:?}",
                id_type.as_str()
            ),
        }
    }
}

impl From<&IdVal> for Value {
    fn from(id: &IdVal) -> Self {
        id.0.clone()
    }
}

impl graph::data::graphql::IntoValue for &IdVal {
    fn into_value(self) -> r::Value {
        self.0.clone().into()
    }
}

impl std::fmt::Display for IdVal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Copy, Debug)]
enum IdType {
    String,
    Bytes,
    Int8,
}

impl IdType {
    fn parse(&self, s: &str) -> IdVal {
        let value = match self {
            IdType::String => Value::String(s.to_string()),
            IdType::Bytes => Value::Bytes(s.parse().unwrap()),
            IdType::Int8 => Value::Int8(s.parse().unwrap()),
        };
        IdVal(value)
    }

    fn songs(&self) -> &[&IdVal] {
        lazy_static! {
            static ref SONGS_STRING_VAL: Vec<IdVal> = SONGS_STRING
                .iter()
                .map(|s| IdType::String.parse(s))
                .collect::<Vec<_>>();
            static ref SONGS_BYTES_VAL: Vec<IdVal> = SONGS_BYTES
                .iter()
                .map(|s| IdType::Bytes.parse(s))
                .collect::<Vec<_>>();
            static ref SONGS_INT_VAL: Vec<IdVal> = SONGS_INT
                .iter()
                .map(|s| IdType::Int8.parse(s))
                .collect::<Vec<_>>();
            static ref SONGS_STRING_REF: Vec<&'static IdVal> =
                SONGS_STRING_VAL.iter().collect::<Vec<_>>();
            static ref SONGS_BYTES_REF: Vec<&'static IdVal> =
                SONGS_BYTES_VAL.iter().collect::<Vec<_>>();
            static ref SONGS_INT_REF: Vec<&'static IdVal> =
                SONGS_INT_VAL.iter().collect::<Vec<_>>();
        }
        match self {
            IdType::String => SONGS_STRING_REF.as_slice(),
            IdType::Bytes => SONGS_BYTES_REF.as_slice(),
            IdType::Int8 => SONGS_INT_REF.as_slice(),
        }
    }

    fn medias(&self) -> &[&str] {
        match self {
            IdType::String => MEDIA_STRING.as_slice(),
            IdType::Bytes => MEDIA_BYTES.as_slice(),
            IdType::Int8 => MEDIA_INT.as_slice(),
        }
    }

    fn as_str(&self) -> &str {
        match self {
            IdType::String => "String",
            IdType::Bytes => "Bytes",
            IdType::Int8 => "Int8",
        }
    }

    fn deployment_id(&self) -> &str {
        match self {
            IdType::String => "graphqlTestsQuery",
            IdType::Bytes => "graphqlTestsQueryBytes",
            IdType::Int8 => "graphqlTestsQueryInt8",
        }
    }
}

/// Setup a basic deployment. The test using the deployment must not modify
/// the deployment at all
async fn setup_readonly(store: &Store) -> DeploymentLocator {
    setup(store, "graphqlTestsQuery", BTreeSet::new(), IdType::String).await
}

/// Set up a deployment `id` with the test schema and populate it with test
/// data. If the `id` is the same as `id_type.deployment_id()`, the test
/// must not modify the deployment in any way as these are reused for other
/// tests that expect pristine data
async fn setup(
    store: &Store,
    id: &str,
    features: BTreeSet<SubgraphFeature>,
    id_type: IdType,
) -> DeploymentLocator {
    use test_store::block_store::{self, BLOCK_ONE, BLOCK_TWO, GENESIS_BLOCK};

    /// Make sure we get rid of all subgraphs once for the entire test run
    async fn global_init() {
        lazy_static! {
            static ref STORE_CLEAN: AtomicBool = AtomicBool::new(false);
        }
        if !STORE_CLEAN.load(Ordering::SeqCst) {
            let chain = vec![&*GENESIS_BLOCK, &*BLOCK_ONE, &*BLOCK_TWO];
            block_store::set_chain(chain, NETWORK_NAME).await;
            test_store::remove_subgraphs();
            STORE_CLEAN.store(true, Ordering::SeqCst);
        }
    }

    async fn initialize(
        store: &Store,
        id: DeploymentHash,
        features: BTreeSet<SubgraphFeature>,
        id_type: IdType,
    ) -> DeploymentLocator {
        let schema = test_schema(id.clone(), id_type);
        let manifest = SubgraphManifest::<graph_chain_ethereum::Chain> {
            id: id.clone(),
            spec_version: Version::new(1, 0, 0),
            features,
            description: None,
            repository: None,
            schema: schema.clone(),
            data_sources: vec![],
            graft: None,
            templates: vec![],
            chain: PhantomData,
            indexer_hints: None,
        };

        insert_test_entities(store.subgraph_store().as_ref(), manifest, id_type).await
    }

    global_init().await;
    let id = DeploymentHash::new(id).unwrap();
    let loc = store.subgraph_store().active_locator(&id).unwrap();

    match loc {
        Some(loc) if id_type.deployment_id() == loc.hash.as_str() => loc,
        Some(loc) => {
            test_store::remove_subgraph(&loc.hash);
            initialize(store, id, features, id_type).await
        }
        None => initialize(store, id, features, id_type).await,
    }
}

fn test_schema(id: DeploymentHash, id_type: IdType) -> InputSchema {
    const SCHEMA: &str = "

    type _Schema_
        @fulltext(
            name: \"bandReviewSearch\"
            language: en
            algorithm: proximityRank
            include: [
                {
                    entity: \"BandReview\"
                    fields: [
                        { name: \"body\" }
                    ]
                }
            ]
        )

    type Musician @entity {
        id: ID!
        name: String!
        mainBand: Band
        bands: [Band!]!
        writtenSongs: [Song!]! @derivedFrom(field: \"writtenBy\")
        favoriteCount: Int8!
    }

    type Band @entity {
        id: ID!
        name: String!
        members: [Musician!]! @derivedFrom(field: \"bands\")
        reviews: [BandReview!]! @derivedFrom(field: \"band\")
        originalSongs: [Song!]!
    }

    type Song @entity {
        id: @ID@!
        sid: String!
        title: String!
        writtenBy: Musician!
        publisher: Publisher!
        band: Band @derivedFrom(field: \"originalSongs\")
        reviews: [SongReview!]! @derivedFrom(field: \"song\")
        media: [Media!]!
        release: Release! @derivedFrom(field: \"songs\")
    }

    type SongStat @entity {
        id: @ID@!
        song: Song @derivedFrom(field: \"id\")
        played: Int!
    }

    type Publisher @entity {
        id: Bytes!
    }

    interface Review @entity {
        id: ID!
        body: String!
        author: Author!
    }

    type SongReview implements Review @entity {
        id: ID!
        body: String!
        song: Song
        author: Author!
    }

    type BandReview implements Review @entity {
        id: ID!
        body: String!
        band: Band
        author: Author!
    }

    interface Media {
        id: ID!
        title: String!
        song: Song!
        author: User!
    }

    type Photo implements Media @entity {
        id: ID!
        title: String!
        song: Song! @derivedFrom(field: \"media\")
        author: User!
    }

    type Video implements Media @entity {
        id: ID!
        title: String!
        song: Song! @derivedFrom(field: \"media\")
        author: User!
    }

    interface Release {
        id: ID!
        title: String!
        songs: [Song!]!
    }

    type Single implements Release @entity {
        id: ID!
        title: String!
        # It could be a single song
        # but let's say a Single represents one song + bonus tracks
        songs: [Song!]!
    }

    type Album implements Release @entity {
        id: ID!
        title: String!
        songs: [Song!]!
    }

    interface Author {
        id: ID!
        name: String!
        reviews: [Review!]!
        bandReviews: [BandReview!]!
        songReviews: [SongReview!]!
    }

    type User implements Author @entity {
        id: ID!
        name: String!
        reviews: [Review!]! @derivedFrom(field: \"author\")
        bandReviews: [BandReview!]! @derivedFrom(field: \"author\")
        songReviews: [SongReview!]! @derivedFrom(field: \"author\")
        latestSongReview: SongReview!
        latestBandReview: BandReview!
        latestReview: Review!
        medias: [Media!]! @derivedFrom(field: \"author\")
    }

    type AnonymousUser implements Author @entity {
        id: ID!
        name: String!
        reviews: [Review!]! @derivedFrom(field: \"author\")
        bandReviews: [BandReview!]! @derivedFrom(field: \"author\")
        songReviews: [SongReview!]! @derivedFrom(field: \"author\")
    }
    ";

    InputSchema::parse(&SCHEMA.replace("@ID@", id_type.as_str()), id).expect("Test schema invalid")
}

async fn insert_test_entities(
    store: &impl SubgraphStore,
    manifest: SubgraphManifest<graph_chain_ethereum::Chain>,
    id_type: IdType,
) -> DeploymentLocator {
    fn insert_ops(
        schema: &InputSchema,
        entities: Vec<(&str, Vec<Entity>)>,
    ) -> Vec<EntityOperation> {
        entities
            .into_iter()
            .map(|(typename, entities)| {
                let entity_type = schema.entity_type(typename).unwrap();
                entities.into_iter().map(move |data| EntityOperation::Set {
                    key: entity_type.key(data.id()),
                    data,
                })
            })
            .flatten()
            .collect()
    }

    async fn insert_at(
        insert_ops: Vec<EntityOperation>,
        deployment: &DeploymentLocator,
        block_ptr: BlockPtr,
    ) {
        test_store::transact_and_wait(&STORE.subgraph_store(), deployment, block_ptr, insert_ops)
            .await
            .unwrap();
    }

    let deployment = DeploymentCreate::new(String::new(), &manifest, None);
    let name = SubgraphName::new(manifest.id.as_str()).unwrap();
    let node_id = NodeId::new("test").unwrap();
    let deployment = store
        .create_subgraph_deployment(
            name,
            &manifest.schema,
            deployment,
            node_id,
            NETWORK_NAME.to_string(),
            SubgraphVersionSwitchingMode::Instant,
        )
        .unwrap();

    let s = id_type.songs();
    let md = id_type.medias();
    let is = &manifest.schema;
    let pub1 = &*PUB1;
    let entities0 = vec![
        (
            "Musician",
            vec![
                entity! { is => id: "m1", name: "John", mainBand: "b1", bands: vec!["b1", "b2"], favoriteCount: 10 },
                entity! { is => id: "m2", name: "Lisa", mainBand: "b1", bands: vec!["b1"], favoriteCount: 100 },
            ],
        ),
        ("Publisher", vec![entity! { is => id: pub1 }]),
        (
            "Band",
            vec![
                entity! { is => id: "b1", name: "The Musicians", originalSongs: vec![s[1], s[2]] },
                entity! { is => id: "b2", name: "The Amateurs",  originalSongs: vec![s[1], s[3], s[4]] },
            ],
        ),
        (
            "Song",
            vec![
                entity! { is => id: s[1], sid: "s1", title: "Cheesy Tune",  publisher: pub1, writtenBy: "m1", media: vec![md[1], md[2]] },
                entity! { is => id: s[2], sid: "s2", title: "Rock Tune",    publisher: pub1, writtenBy: "m2", media: vec![md[3], md[4]] },
                entity! { is => id: s[3], sid: "s3", title: "Pop Tune",     publisher: pub1, writtenBy: "m1", media: vec![md[5]] },
                entity! { is => id: s[4], sid: "s4", title: "Folk Tune",    publisher: pub1, writtenBy: "m3", media: vec![md[6]] },
            ],
        ),
        (
            "User",
            vec![
                entity! { is => id: "u1", name: "User 1", latestSongReview: "r3", latestBandReview: "r1", latestReview: "r3" },
            ],
        ),
        (
            "SongStat",
            vec![
                entity! { is => id: s[1], played: 10 },
                entity! { is => id: s[2], played: 15 },
            ],
        ),
        (
            "BandReview",
            vec![
                entity! { is => id: "r1", body: "Bad musicians",        band: "b1", author: "u1" },
                entity! { is => id: "r2", body: "Good amateurs",        band: "b2", author: "u2" },
                entity! { is => id: "r5", body: "Very Bad musicians",   band: "b1", author: "u3" },
            ],
        ),
        (
            "SongReview",
            vec![
                entity! { is => id: "r3", body: "Bad",                  song: s[2], author: "u1" },
                entity! { is => id: "r4", body: "Good",                 song: s[3], author: "u2" },
                entity! { is => id: "r6", body: "Very Bad",             song: s[2], author: "u3" },
            ],
        ),
        (
            "User",
            vec![
                entity! { is => id: "u1", name: "Baden",        latestSongReview: "r3", latestBandReview: "r1", latestReview: "r1" },
                entity! { is => id: "u2", name: "Goodwill",     latestSongReview: "r4", latestBandReview: "r2", latestReview: "r2" },
            ],
        ),
        (
            "AnonymousUser",
            vec![
                entity! { is => id: "u3", name: "Anonymous 3",  latestSongReview: "r6", latestBandReview: "r5", latestReview: "r5" },
            ],
        ),
        (
            "Photo",
            vec![
                entity! { is => id: md[1],   title: "Cheesy Tune Single Cover",  author: "u1" },
                entity! { is => id: md[3],   title: "Rock Tune Single Cover",    author: "u1" },
                entity! { is => id: md[5],   title: "Pop Tune Single Cover",     author: "u1" },
            ],
        ),
        (
            "Video",
            vec![
                entity! { is => id: md[2],   title: "Cheesy Tune Music Video",   author: "u2" },
                entity! { is => id: md[4],   title: "Rock Tune Music Video",     author: "u2" },
                entity! { is => id: md[6],   title: "Folk Tune Music Video",     author: "u2" },
            ],
        ),
        (
            "Album",
            vec![entity! { is => id: "rl1",   title: "Pop and Folk",    songs: vec![s[3], s[4]] }],
        ),
        (
            "Single",
            vec![
                entity! { is => id: "rl2",  title: "Rock",           songs: vec![s[2]] },
                entity! { is => id: "rl3",  title: "Cheesy",         songs: vec![s[1]] },
                entity! { is => id: "rl4",  title: "Silence",        songs: Vec::<graph::prelude::Value>::new() },
            ],
        ),
    ];
    let entities0 = insert_ops(&manifest.schema, entities0);

    let entities1 = vec![(
        "Musician",
        vec![
            entity! { is => id: "m3", name: "Tom", mainBand: "b2", bands: vec!["b1", "b2"], favoriteCount: 5 },
            entity! { is => id: "m4", name: "Valerie", bands: Vec::<String>::new(), favoriteCount: 20 },
        ],
    )];
    let entities1 = insert_ops(&manifest.schema, entities1);

    insert_at(entities0, &deployment, GENESIS_PTR.clone()).await;
    insert_at(entities1, &deployment, BLOCK_ONE.clone()).await;
    deployment
}

async fn execute_query(loc: &DeploymentLocator, query: &str) -> QueryResult {
    let query = graphql_parser::parse_query(query)
        .expect("invalid test query")
        .into_static();
    execute_query_document_with_variables(&loc.hash, query, None).await
}

async fn execute_query_document_with_variables(
    id: &DeploymentHash,
    query: q::Document,
    variables: Option<QueryVariables>,
) -> QueryResult {
    let runner = Arc::new(GraphQlRunner::new(
        &LOGGER,
        STORE.clone(),
        SUBSCRIPTION_MANAGER.clone(),
        LOAD_MANAGER.clone(),
        METRICS_REGISTRY.clone(),
    ));
    let target = QueryTarget::Deployment(id.clone(), Default::default());
    let query = Query::new(query, variables, false);

    runner
        .run_query_with_complexity(query, target, None, None, None, None)
        .await
        .first()
        .unwrap()
        .duplicate()
}

async fn first_result(f: QueryResults) -> QueryResult {
    f.first().unwrap().duplicate()
}

/// Extract the data from a `QueryResult`, and panic if it has errors
macro_rules! extract_data {
    ($result: expr) => {
        match $result.to_result() {
            Err(errors) => panic!("Unexpected errors return for query: {:#?}", errors),
            Ok(data) => data,
        }
    };
}

struct QueryArgs {
    query: String,
    variables: Option<QueryVariables>,
    max_complexity: Option<u64>,
}

impl From<&str> for QueryArgs {
    fn from(query: &str) -> Self {
        QueryArgs {
            query: query.to_owned(),
            variables: None,
            max_complexity: None,
        }
    }
}

impl From<String> for QueryArgs {
    fn from(query: String) -> Self {
        QueryArgs {
            query,
            variables: None,
            max_complexity: None,
        }
    }
}

impl From<(&str, r::Value)> for QueryArgs {
    fn from((query, vars): (&str, r::Value)) -> Self {
        let vars = match vars {
            r::Value::Object(map) => map,
            _ => panic!("vars must be an object"),
        };
        let vars = QueryVariables::new(HashMap::from_iter(
            vars.into_iter().map(|(k, v)| (k.to_string(), v)),
        ));
        QueryArgs {
            query: query.to_owned(),
            variables: Some(vars),
            max_complexity: None,
        }
    }
}

/// Run a GraphQL query against the `test_schema` and call the `test`
/// function with the result. The query is actually run twice: once against
/// the test schema where the `id` of `Song` and `SongStats` has type
/// `String`, and once where it has type `Bytes`. The second argument to
/// `test` indicates which type is being used for the id.
///
/// The query can contain placeholders `@S1@` .. `@S4@` which will be
/// replaced with the id's of songs 1 through 4 before running the query.
fn run_query<F>(args: impl Into<QueryArgs>, test: F)
where
    F: Fn(QueryResult, IdType) + Send + 'static,
{
    let QueryArgs {
        query,
        variables,
        max_complexity,
    } = args.into();
    run_test_sequentially(move |store| async move {
        for id_type in [IdType::String, IdType::Bytes, IdType::Int8] {
            let name = id_type.deployment_id();

            let deployment = setup(store.as_ref(), name, BTreeSet::new(), id_type).await;

            let mut query = query.clone();
            for (i, id) in id_type.songs().iter().enumerate() {
                let pat = format!("@S{i}@");
                let repl = id.as_gql(id_type);
                query = query.replace(&pat, &repl);
            }
            dbg!(&query);

            let result = {
                let id = &deployment.hash;
                let query = graphql_parser::parse_query(&query)
                    .expect("Invalid test query")
                    .into_static();
                let variables = variables.clone();
                let runner = Arc::new(GraphQlRunner::new(
                    &LOGGER,
                    STORE.clone(),
                    SUBSCRIPTION_MANAGER.clone(),
                    LOAD_MANAGER.clone(),
                    METRICS_REGISTRY.clone(),
                ));
                let target = QueryTarget::Deployment(id.clone(), Default::default());
                let query = Query::new(query, variables, false);

                runner
                    .run_query_with_complexity(query, target, max_complexity, None, None, None)
                    .await
                    .first()
                    .unwrap()
                    .duplicate()
            };
            test(result, id_type);
        }
    })
}

/// Helper to run a subscription
async fn run_subscription(
    store: &Arc<Store>,
    query: &str,
    max_complexity: Option<u64>,
) -> Result<SubscriptionResult, SubscriptionError> {
    let deployment = setup_readonly(store.as_ref()).await;
    let logger = Logger::root(slog::Discard, o!());
    let query_store = store
        .query_store(
            QueryTarget::Deployment(deployment.hash.clone(), Default::default()),
            true,
        )
        .await
        .unwrap();

    let query = Query::new(
        graphql_parser::parse_query(query).unwrap().into_static(),
        None,
        false,
    );
    let options = SubscriptionExecutionOptions {
        logger: logger.clone(),
        store: query_store.clone(),
        subscription_manager: SUBSCRIPTION_MANAGER.clone(),
        timeout: None,
        max_complexity,
        max_depth: 100,
        max_first: std::u32::MAX,
        max_skip: std::u32::MAX,
        graphql_metrics: graphql_metrics(),
        load_manager: LOAD_MANAGER.clone(),
    };
    let schema = STORE
        .subgraph_store()
        .api_schema(&deployment.hash, &Default::default())
        .unwrap();

    execute_subscription(Subscription { query }, schema, options)
}

#[test]
fn can_query_one_to_one_relationship() {
    const QUERY: &str = "
    query {
        musicians(first: 100, orderBy: id) {
            name
            mainBand {
                name
            }
            favoriteCount
        }
        songStats(first: 100, orderBy: id) {
            id
            song {
              id
              title
            }
            played
        }
    }
    ";

    run_query(QUERY, |result, id_type| {
        let s = id_type.songs();
        let exp = object! {
            musicians: vec![
                object! { name: "John", mainBand: object! { name: "The Musicians" }, favoriteCount: "10" },
                object! { name: "Lisa", mainBand: object! { name: "The Musicians" }, favoriteCount: "100" },
                object! { name: "Tom",  mainBand: object! { name: "The Amateurs" }, favoriteCount: "5" },
                object! { name: "Valerie", mainBand: r::Value::Null, favoriteCount: "20" }
            ],
            songStats: vec![
                object! {
                    id: s[1],
                    song: object! { id: s[1], title: "Cheesy Tune" },
                    played: 10,
                },
                object! {
                    id: s[2],
                    song: object! { id: s[2], title: "Rock Tune" },
                    played: 15
                }
            ]
        };
        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_one_to_many_relationships_in_both_directions() {
    const QUERY: &str = "
    query {
        musicians(first: 100, orderBy: id) {
            name
            writtenSongs(first: 100, orderBy: id) {
                title
                writtenBy { name }
            }
        }
    }";

    run_query(QUERY, |result, _| {
        fn song(title: &str, author: &str) -> r::Value {
            object! {
                title: title,
                writtenBy: object! { name: author }
            }
        }

        let exp = object! {
            musicians: vec![
                object! {
                    name: "John",
                    writtenSongs: vec![
                        song("Cheesy Tune", "John"),
                        song("Pop Tune", "John"),
                    ]
                },
                object! {
                    name: "Lisa", writtenSongs: vec![ song("Rock Tune", "Lisa") ]
                },
                object! {
                    name: "Tom", writtenSongs: vec![ song("Folk Tune", "Tom") ]
                },
                object! {
                    name: "Valerie", writtenSongs: Vec::<String>::new()
                },
            ]
        };

        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_many_to_many_relationship() {
    const QUERY: &str = "
    query {
        musicians(first: 100, orderBy: id) {
            name
            bands(first: 100, orderBy: id) {
                name
                members(first: 100, orderBy: id) {
                    name
                }
            }
        }
    }";

    run_query(QUERY, |result, _| {
        fn members(names: Vec<&str>) -> Vec<r::Value> {
            names
                .into_iter()
                .map(|name| object! { name: name })
                .collect()
        }

        let the_musicians = object! {
            name: "The Musicians",
            members: members(vec!["John", "Lisa", "Tom"])
        };

        let the_amateurs = object! {
            name: "The Amateurs",
            members: members(vec![ "John", "Tom" ])
        };

        let exp = object! {
            musicians: vec![
                object! { name: "John", bands: vec![ the_musicians.clone(), the_amateurs.clone() ]},
                object! { name: "Lisa", bands: vec![ the_musicians.clone() ] },
                object! { name: "Tom", bands: vec![ the_musicians, the_amateurs ] },
                object! { name: "Valerie", bands: Vec::<String>::new() }
            ]
        };

        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_with_fulltext_search() {
    const QUERY: &str = "
    query {
        bandReviewSearch(text: \"musicians\") {
            id
            body
            author {
                name
            }
        }
    }";

    run_query(QUERY, |result, _| {
        let exp = object! {
            bandReviewSearch: vec![
                object! { id: "r1", body: "Bad musicians", author: object! { name: "Baden" } },
                object! { id: "r5", body: "Very Bad musicians", author: object! { name: "Anonymous 3" } },
            ]
        };
        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_with_fulltext_search_filter() {
    const QUERY: &str = "
    query {
        bandReviewSearch(text: \"musicians\", where: { author_: { name: \"Anonymous 3\" } }) {
            id
            body
            author {
                name
            }
        }
    }";

    run_query(QUERY, |result, _| {
        let exp = object! {
            bandReviewSearch: vec![
                object! { id: "r5", body: "Very Bad musicians", author: object! { name: "Anonymous 3" } },
            ]
        };
        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_with_sorting_by_child_entity() {
    const QUERY: &str = "
    query {
        desc: musicians(first: 100, orderBy: mainBand__name, orderDirection: desc) {
            name
            mainBand {
                name
            }
        }
        asc: musicians(first: 100, orderBy: mainBand__name, orderDirection: asc) {
            name
            mainBand {
                name
            }
        }
    }";

    run_query(QUERY, |result, _| {
        let exp = object! {
            desc: vec![
                object! { name: "Valerie", mainBand: r::Value::Null },
                object! { name: "Lisa", mainBand: object! { name: "The Musicians" } },
                object! { name: "John", mainBand: object! { name: "The Musicians" } },
                object! { name: "Tom",  mainBand: object! { name: "The Amateurs"} },
                ],
            asc: vec![
                object! { name: "Tom",  mainBand: object! { name: "The Amateurs"} },
                object! { name: "John", mainBand: object! { name: "The Musicians" } },
                object! { name: "Lisa", mainBand: object! { name: "The Musicians" } },
                object! { name: "Valerie", mainBand: r::Value::Null },
                ]
        };

        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_with_sorting_by_derived_child_entity() {
    const QUERY: &str = "
    query {
        desc: songStats(first: 100, orderBy: song__title, orderDirection: desc) {
            id
            song {
              id
              title
            }
            played
        }
        asc: songStats(first: 100, orderBy: song__title, orderDirection: asc) {
            id
            song {
              id
              title
            }
            played
        }
    }";

    run_query(QUERY, |result, id_type| {
        let s = id_type.songs();
        let exp = object! {
            desc: vec![
                object! {
                    id: s[2],
                    song: object! { id: s[2], title: "Rock Tune" },
                    played: 15
                },
                object! {
                    id: s[1],
                    song: object! { id: s[1], title: "Cheesy Tune" },
                    played: 10,
                }
            ],
            asc: vec![
                object! {
                    id: s[1],
                    song: object! { id: s[1], title: "Cheesy Tune" },
                    played: 10,
                },
                object! {
                    id: s[2],
                    song: object! { id: s[2], title: "Rock Tune" },
                    played: 15
                }
            ]
        };

        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_with_sorting_by_child_entity_id() {
    const QUERY: &str = "
    query {
        desc: bandReviews(first: 100, orderBy: author__id, orderDirection: desc) {
            body
            author {
                name
            }
        }
        asc: bandReviews(first: 100, orderBy: author__id, orderDirection: asc) {
            body
            author {
                name
            }
        }
    }";

    run_query(QUERY, |result, _| {
        let exp = object! {
            desc: vec![
                object! { body: "Very Bad musicians",   author: object! { name: "Anonymous 3"  } },
                object! { body: "Good amateurs",        author: object! { name: "Goodwill" } },
                object! { body: "Bad musicians",        author: object! { name: "Baden" } },
                ],
            asc: vec![
                object! { body: "Bad musicians",        author: object! { name: "Baden" } },
                object! { body: "Good amateurs",        author: object! { name: "Goodwill" } },
                object! { body: "Very Bad musicians",   author: object! { name: "Anonymous 3"  } },
                ]
        };

        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_with_sorting_by_derived_child_entity_id() {
    const QUERY: &str = "
    query {
        desc: songStats(first: 100, orderBy: song__id, orderDirection: desc) {
            id
            song {
              id
              title
            }
            played
        }
        asc: songStats(first: 100, orderBy: song__id, orderDirection: asc) {
            id
            song {
              id
              title
            }
            played
        }
    }";

    run_query(QUERY, |result, id_type| {
        let s = id_type.songs();
        let exp = object! {
            desc: vec![
                object! {
                    id: s[2],
                    song: object! { id: s[2], title: "Rock Tune" },
                    played: 15
                },
                object! {
                    id: s[1],
                    song: object! { id: s[1], title: "Cheesy Tune" },
                    played: 10,
                }
            ],
            asc: vec![
                object! {
                    id: s[1],
                    song: object! { id: s[1], title: "Cheesy Tune" },
                    played: 10,
                },
                object! {
                    id: s[2],
                    song: object! { id: s[2], title: "Rock Tune" },
                    played: 15
                }
            ]
        };

        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_with_sorting_by_child_interface() {
    const QUERY: &str = "
    query {
        desc: songReviews(first: 100, orderBy: author__name, orderDirection: desc) {
            body
            author {
                name
            }
        }
        asc: songReviews(first: 100, orderBy: author__name, orderDirection: asc) {
            body
            author {
                name
            }
        }
    }";

    run_query(QUERY, |result, _| {
        let exp = object! {
            desc: vec![
                object! { body: "Good", author: object! { name: "Goodwill" } },
                object! { body: "Bad", author: object! { name: "Baden" } },
                object! { body: "Very Bad", author: object! { name: "Anonymous 3" } },
                ],
            asc: vec![
                object! { body: "Very Bad", author: object! { name: "Anonymous 3" } },
                object! { body: "Bad", author: object! { name: "Baden" } },
                object! { body: "Good", author: object! { name: "Goodwill" } },
                ]
        };

        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_interface_with_sorting_by_child_entity() {
    const QUERY: &str = "
    query {
        desc: medias(first: 100, orderBy: author__name, orderDirection: desc) {
            title
            author {
                name
            }
        }
        asc: medias(first: 100, orderBy: author__name, orderDirection: asc) {
            title
            author {
                name
            }
        }
    }";

    run_query(QUERY, |result, _| {
        let author1 = object! { name: "Baden" };
        let author2 = object! { name: "Goodwill" };
        let desc_medias = vec![
            object! { title: "Folk Tune Music Video",       author: author2.clone() },
            object! { title: "Rock Tune Music Video",       author: author2.clone() },
            object! { title: "Cheesy Tune Music Video",     author: author2.clone() },
            object! { title: "Pop Tune Single Cover",       author: author1.clone() },
            object! { title: "Rock Tune Single Cover",      author: author1.clone() },
            object! { title: "Cheesy Tune Single Cover",    author: author1.clone() },
        ];
        let mut asc_medias = desc_medias.clone();

        asc_medias.reverse();

        let exp = object! {
            desc: desc_medias,
            asc: asc_medias,
        };

        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    });
}

#[test]
fn can_query_interface_with_sorting_by_derived_child_entity() {
    const QUERY: &str = "
    query {
        desc: medias(first: 100, orderBy: song__title, orderDirection: desc) {
            title
            song {
                title
            }
        }
        asc: medias(first: 100, orderBy: song__title, orderDirection: asc) {
            title
            song {
                title
            }
        }
    }";

    run_query(QUERY, |result, _| {
        let exp = object! {
            desc: vec![
                object! { title: "Rock Tune Music Video", song : object! { title: "Rock Tune" } },
                object! { title: "Rock Tune Single Cover", song : object! { title: "Rock Tune" } },
                object! { title: "Pop Tune Single Cover", song : object! { title: "Pop Tune" } },
                object! { title: "Folk Tune Music Video", song : object! { title: "Folk Tune" } },
                object! { title: "Cheesy Tune Music Video", song : object! { title: "Cheesy Tune" } },
                object! { title: "Cheesy Tune Single Cover", song : object! { title: "Cheesy Tune" } },
            ],
            asc: vec![
                object! { title: "Cheesy Tune Single Cover", song : object! { title: "Cheesy Tune" } },
                object! { title: "Cheesy Tune Music Video", song : object! { title: "Cheesy Tune" } },
                object! { title: "Folk Tune Music Video", song : object! { title: "Folk Tune" } },
                object! { title: "Pop Tune Single Cover", song : object! { title: "Pop Tune" } },
                object! { title: "Rock Tune Single Cover", song : object! { title: "Rock Tune" } },
                object! { title: "Rock Tune Music Video", song : object! { title: "Rock Tune" } },
            ]
        };

        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    });
}

#[test]
fn can_query_with_child_filter_on_list_type_field() {
    const QUERY: &str = "
    query {
        musicians(first: 100, orderBy: id, where: { bands_: { name: \"The Amateurs\" } }) {
            name
            bands(first: 100, orderBy: id) {
                name
            }
        }
    }";

    run_query(QUERY, |result, _| {
        let the_musicians = object! {
            name: "The Musicians",
        };

        let the_amateurs = object! {
            name: "The Amateurs",
        };

        let exp = object! {
            musicians: vec![
                object! { name: "John", bands: vec![ the_musicians.clone(), the_amateurs.clone() ]},
                object! { name: "Tom", bands: vec![ the_musicians, the_amateurs ] },
            ]
        };

        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_with_child_filter_on_derived_list_type_field() {
    const QUERY: &str = "
    query {
        musicians(first: 100, orderBy: id, where: { writtenSongs_: { title_contains: \"Rock\" } }) {
            name
        }
    }";

    run_query(QUERY, |result, _| {
        let exp = object! {
            musicians: vec![
                object! { name: "Lisa" },
            ]
        };

        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_with_child_filter_on_named_type_field() {
    const QUERY: &str = "
    query {
        musicians(first: 100, orderBy: id, where: { mainBand_: { name_contains: \"The Amateurs\" } }) {
            name
            mainBand {
                id
            }
        }
    }";

    run_query(QUERY, |result, _| {
        let exp = object! {
            musicians: vec![
                object! { name: "Tom", mainBand: object! { id: "b2"} }
            ]
        };

        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_with_child_filter_on_derived_named_type_field() {
    const QUERY: &str = "
    query {
        songs(first: 100, orderBy: id, where: { band_: { name_contains: \"The Musicians\" } }) {
            title
        }
    }";

    run_query(QUERY, |result, _| {
        let exp = object! {
            songs: vec![
                object! { title: "Cheesy Tune" },
                object! { title: "Rock Tune" },
            ]
        };

        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_an_interface_with_child_filter_on_named_type_field() {
    const QUERY: &str = "
    query {
        reviews(first: 100, orderBy: id, where: { author_: { name_starts_with: \"Good\" } }) {
            body
            author {
                name
            }
        }
    }";

    run_query(QUERY, |result, _| {
        let exp = object! {
            reviews: vec![
                object! { body: "Good amateurs", author: object! { name: "Goodwill" } },
                object! { body: "Good", author: object! { name: "Goodwill" } },
            ]
        };

        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_with_child_filter_on_derived_interface_list_field() {
    const QUERY: &str = "
    query {
        users(first: 100, orderBy: id, where: { reviews_: { body_starts_with: \"Good\" } }) {
            name
            reviews {
                body
            }
        }
    }";

    run_query(QUERY, |result, _| {
        let exp = object! {
            users: vec![
                object! { name: "Goodwill", reviews: vec![ object! { body: "Good amateurs" }, object! { body: "Good" } ] },
            ]
        };

        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_entity_by_child_entity_field() {
    const QUERY: &str = "
    query {
        users(first: 100, orderBy: id, where: { latestSongReview_: { body_starts_with: \"Good\" } }) {
            name
            latestSongReview {
                body
            }
        }
    }";

    run_query(QUERY, |result, _| {
        let exp = object! {
            users: vec![
                object! { name: "Goodwill", latestSongReview: object! { body: "Good" } },
            ]
        };

        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_entity_by_child_interface_field() {
    const QUERY: &str = "
    query {
        users(first: 100, orderBy: id, where: { latestReview_: { body_starts_with: \"Good\" } }) {
            name
            latestReview {
                body
            }
        }
    }";

    run_query(QUERY, |result, _| {
        let exp = object! {
            users: vec![
                object! { name: "Goodwill", latestReview: object! { body: "Good amateurs" } },
            ]
        };

        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_interface_by_child_entity_field() {
    const QUERY: &str = "
    query {
        reviews(first: 100, orderBy: id, where: { author_: { name_starts_with: \"Good\" } }) {
            body
            author {
                name
            }
        }
    }";

    run_query(QUERY, |result, _| {
        let exp = object! {
            reviews: vec![
                object! { body: "Good amateurs", author: object! { name: "Goodwill" } },
                object! { body: "Good", author: object! { name: "Goodwill" } },
            ]
        };

        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_entity_by_child_interface_derived_field() {
    const QUERY: &str = "
    query {
        songs(first: 100, orderBy: id, where: { release_: { title_starts_with: \"Pop\" } }) {
            title
            release {
                title
            }
        }
    }";

    run_query(QUERY, |result, _| {
        let exp = object! {
            songs: vec![
                object! { title: "Pop Tune", release: object! { title: "Pop and Folk" } },
                object! { title: "Folk Tune", release: object! { title: "Pop and Folk" } },
            ]
        };

        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_interface_by_child_entity_derived_field() {
    const QUERY: &str = "
    query {
        medias(first: 100, orderBy: id, where: { song_: { title_starts_with: \"Folk\" } }) {
            title
            song {
                title
            }
        }
    }";

    run_query(QUERY, |result, _| {
        let exp = object! {
            medias: vec![
                object! { title: "Folk Tune Music Video", song: object! { title: "Folk Tune" } },
            ]
        };

        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_entity_by_child_interface_list_field() {
    const QUERY: &str = "
    query {
        songs(first: 100, orderBy: id, where: { media_: { title_starts_with: \"Cheesy Tune\" } }) {
            title
            media {
                title
            }
        }
    }";

    run_query(QUERY, |result, _| {
        let exp = object! {
            songs: vec![
                object! { title: "Cheesy Tune", media: vec![
                    object! { title: "Cheesy Tune Single Cover" },
                    object! { title: "Cheesy Tune Music Video" }
                ] },
            ]
        };

        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_entity_by_child_interface_list_derived_field() {
    const QUERY: &str = "
    query {
        songs(first: 100, orderBy: id, where: { reviews_: { body_starts_with: \"Good\" } }) {
            title
            reviews {
                body
            }
        }
    }";

    run_query(QUERY, |result, _| {
        let exp = object! {
            songs: vec![
                object! { title: "Pop Tune", reviews: vec![
                    object! { body: "Good" },
                ] },
            ]
        };

        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn root_fragments_are_expanded() {
    const QUERY: &str = r#"
    fragment Musicians on Query {
        musicians(first: 100, where: { name: "Tom" }) {
            name
        }
    }
    query MyQuery {
        ...Musicians
    }"#;

    run_query(QUERY, |result, _| {
        let exp = object! { musicians: vec![ object! { name: "Tom" }]};
        assert_eq!(extract_data!(result), Some(exp));
    })
}

#[test]
fn query_variables_are_used() {
    const QUERY: &str = "
    query musicians($where: Musician_filter!) {
      musicians(first: 100, where: $where) {
        name
      }
    }";

    run_query(
        (QUERY, object![ where: object! { name: "Tom"} ]),
        |result, _| {
            let exp = object! {
                musicians: vec![ object! { name: "Tom" }]
            };
            let data = extract_data!(result).unwrap();
            assert_eq!(data, exp);
        },
    );
}

#[test]
fn mixed_parent_child_id() {
    // Check that any combination of parent and child id type (String or
    // Bytes) works in queries

    // `Publisher` has `id` of type `Bytes`, which used to lead to
    // `NonNullError` when `Song` used `String`
    const QUERY: &str = "
    query amxx {
        songs(first: 2) {
            publisher { id }
        }
    }";

    run_query(QUERY, |result, _| {
        let exp = object! {
            songs: vec![
                object! { publisher: object! { id: &*PUB1 } },
                object! { publisher: object! { id: &*PUB1 } }
            ]
        };
        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    });

    const QUERY2: &str = "
    query bytes_string {
        songs(first: 2) {
            writtenBy { id }
        }
    }
    ";
    run_query(QUERY2, |result, _| {
        let exp = object! {
            songs: vec![
                object! { writtenBy: object! { id: "m1" } },
                object! { writtenBy: object! { id: "m2" } }
            ]
        };
        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn skip_directive_works_with_query_variables() {
    const QUERY: &str = "
    query musicians($skip: Boolean!) {
      musicians(first: 100, orderBy: id) {
        id @skip(if: $skip)
        name
      }
    }
";

    run_query((QUERY, object! { skip: true }), |result, _| {
        // Assert that only names are returned
        let musicians: Vec<_> = ["John", "Lisa", "Tom", "Valerie"]
            .into_iter()
            .map(|name| object! { name: name })
            .collect();
        let exp = object! { musicians: musicians };
        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    });

    run_query((QUERY, object! { skip: false }), |result, _| {
        // Assert that IDs and names are returned
        let exp = object! {
            musicians: vec![
                object! { id: "m1", name: "John" },
                object! { id: "m2", name: "Lisa"},
                object! { id: "m3", name: "Tom" },
                object! { id: "m4", name: "Valerie" }
            ]
        };
        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    });
}

#[test]
fn include_directive_works_with_query_variables() {
    const QUERY: &str = "
    query musicians($include: Boolean!) {
      musicians(first: 100, orderBy: id) {
        id @include(if: $include)
        name
      }
    }
";

    run_query((QUERY, object! { include: true }), |result, _| {
        // Assert that IDs and names are returned
        let exp = object! {
            musicians: vec![
                object! { id: "m1", name: "John" },
                object! { id: "m2", name: "Lisa"},
                object! { id: "m3", name: "Tom" },
                object! { id: "m4", name: "Valerie" }
            ]
        };
        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    });

    run_query((QUERY, object! { include: false }), |result, _| {
        // Assert that only names are returned
        let musicians: Vec<_> = ["John", "Lisa", "Tom", "Valerie"]
            .into_iter()
            .map(|name| object! { name: name })
            .collect();
        let exp = object! { musicians: musicians };
        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    });
}

#[test]
fn query_complexity() {
    const QUERY1: &str = "query {
        musicians(orderBy: id) {
            name
            bands(first: 100, orderBy: id) {
                name
                members(first: 100, orderBy: id) {
                    name
                }
            }
        }
    }";
    let args = QueryArgs {
        query: QUERY1.to_owned(),
        variables: None,
        max_complexity: Some(1_010_100),
    };
    run_query(args, |result, _| {
        // This query is exactly at the maximum complexity.
        assert!(!result.has_errors());
    });

    const QUERY2: &str = "query {
        musicians(orderBy: id) {
            name
            bands(first: 100, orderBy: id) {
                name
                members(first: 100, orderBy: id) {
                    name
                }
            }
        }
        __schema {
            types {
                name
            }
        }
    }";
    let args = QueryArgs {
        query: QUERY2.to_owned(),
        variables: None,
        max_complexity: Some(1_010_100),
    };
    run_query(args, |result, _| {
        // The extra introspection causes the complexity to go over.
        match result.to_result().unwrap_err()[0] {
            QueryError::ExecutionError(QueryExecutionError::TooComplex(1_010_200, _)) => (),
            _ => panic!("did not catch complexity"),
        };
    })
}

#[test]
fn query_complexity_subscriptions() {
    run_test_sequentially(|store| async move {
        const QUERY1: &str = "subscription {
                musicians(orderBy: id) {
                    name
                    bands(first: 100, orderBy: id) {
                        name
                        members(first: 100, orderBy: id) {
                            name
                        }
                    }
                }
            }";
        let max_complexity = Some(1_010_100);

        // This query is exactly at the maximum complexity.
        // FIXME: Not collecting the stream because that will hang the test.
        let _ignore_stream = run_subscription(&store, QUERY1, max_complexity)
            .await
            .unwrap();

        const QUERY2: &str = "subscription {
                musicians(orderBy: id) {
                    name
                    t1: bands(first: 100, orderBy: id) {
                        name
                        members(first: 100, orderBy: id) {
                            name
                        }
                    }
                    t2: bands(first: 200, orderBy: id) {
                      name
                      members(first: 100, orderBy: id) {
                          name
                      }
                  }
                }
            }";

        let result = run_subscription(&store, QUERY2, max_complexity).await;

        match result {
            Err(SubscriptionError::GraphQLError(e)) => match &e[0] {
                QueryExecutionError::TooComplex(3_030_100, _) => (), // Expected
                e => panic!("did not catch complexity: {:?}", e),
            },
            _ => panic!("did not catch complexity"),
        }
    })
}

#[test]
fn instant_timeout() {
    run_test_sequentially(|store| async move {
        let deployment = setup_readonly(store.as_ref()).await;
        let query = Query::new(
            graphql_parser::parse_query("query { musicians(first: 100) { name } }")
                .unwrap()
                .into_static(),
            None,
            false,
        );

        match first_result(
            execute_subgraph_query_with_deadline(
                query,
                QueryTarget::Deployment(deployment.hash, Default::default()),
                Some(Instant::now()),
            )
            .await,
        )
        .await
        .to_result()
        .unwrap_err()[0]
        {
            QueryError::ExecutionError(QueryExecutionError::Timeout) => (), // Expected
            _ => panic!("did not time out"),
        };
    })
}

#[test]
fn variable_defaults() {
    const QUERY: &str = "
    query musicians($orderDir: OrderDirection = desc) {
      bands(first: 2, orderBy: id, orderDirection: $orderDir) {
        id
      }
    }
";

    run_query((QUERY, object! {}), |result, _| {
        let exp = object! {
            bands: vec![
                object! { id: "b2" },
                object! { id: "b1" }
            ]
        };
        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    });

    run_query(
        (QUERY, object! { orderDir: r::Value::Null }),
        |result, _| {
            let exp = object! {
                bands: vec![
                    object! { id: "b1" },
                    object! { id: "b2" }
                ]
            };
            let data = extract_data!(result).unwrap();
            assert_eq!(data, exp);
        },
    )
}

#[test]
fn skip_is_nullable() {
    const QUERY: &str = "
    query musicians {
      musicians(orderBy: id, skip: null) {
        name
      }
    }
";

    run_query(QUERY, |result, _| {
        let musicians: Vec<_> = ["John", "Lisa", "Tom", "Valerie"]
            .into_iter()
            .map(|name| object! { name: name })
            .collect();
        let exp = object! { musicians: musicians };
        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn first_is_nullable() {
    const QUERY: &str = "
    query musicians {
      musicians(first: null, orderBy: id) {
        name
      }
    }
";

    run_query(QUERY, |result, _| {
        let musicians: Vec<_> = ["John", "Lisa", "Tom", "Valerie"]
            .into_iter()
            .map(|name| object! { name: name })
            .collect();
        let exp = object! { musicians: musicians };
        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn nested_variable() {
    const QUERY: &str = "
    query musicians($name: String) {
      musicians(first: 100, where: { name: $name }) {
        name
      }
    }
";

    run_query((QUERY, object! { name: "Lisa" }), |result, _| {
        let exp = object! {
            musicians: vec! { object! { name: "Lisa" }}
        };
        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn ambiguous_derived_from_result() {
    const QUERY: &str = "{ songs(first: 100, orderBy: id) { id band { id } } }";

    run_query(QUERY, |result, _| {
        match &result.to_result().unwrap_err()[0] {
            QueryError::ExecutionError(QueryExecutionError::AmbiguousDerivedFromResult(
                pos,
                derived_from_field,
                target_type,
                target_field,
            )) => {
                assert_eq!(
                    pos,
                    &Pos {
                        line: 1,
                        column: 39
                    }
                );
                assert_eq!(derived_from_field.as_str(), "band");
                assert_eq!(target_type.as_str(), "Band");
                assert_eq!(target_field.as_str(), "originalSongs");
            }
            e => panic!("expected AmbiguousDerivedFromResult error, got {}", e),
        }
    })
}

#[test]
fn can_filter_by_relationship_fields() {
    const QUERY: &str = "
    query {
        musicians(orderBy: id, where: { mainBand: \"b2\" }) {
            id name
            mainBand { id }
        }
        bands(orderBy: id, where: { originalSongs: [@S1@, @S3@, @S4@] }) {
            id name
            originalSongs { id }
        }
    }
    ";

    run_query(QUERY, |result, id_type| {
        let s = id_type.songs();

        let exp = object! {
            musicians: vec![
                object! { id: "m3", name: "Tom", mainBand: object! { id: "b2"} }
            ],
            bands: vec![
                object! {
                    id: "b2",
                    name: "The Amateurs",
                    originalSongs: vec! [
                        object! { id: s[1] },
                        object! { id: s[3] },
                        object! { id: s[4] }
                    ]
                }
            ]
        };

        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn cannot_filter_by_derved_relationship_fields() {
    const QUERY: &str = "
    query {
        musicians(orderBy: id, where: { writtenSongs: [@S1@] }) {
            id name
            mainBand { id }
        }
    }
    ";

    run_query(QUERY, |result, _id_type| {
        match &result.to_result().unwrap_err()[0] {
            // With validations
            QueryError::ExecutionError(QueryExecutionError::ValidationError(_, error_message)) => {
                assert_eq!(
                    error_message,
                    "Field \"writtenSongs\" is not defined by type \"Musician_filter\"."
                );
            }
            // Without validations
            QueryError::ExecutionError(QueryExecutionError::InvalidArgumentError(
                _pos,
                error_message,
                _value,
            )) => {
                assert_eq!(error_message, "where");
            }
            e => panic!("expected a runtime/validation error, got {:?}", e),
        };
    })
}

#[test]
fn subscription_gets_result_even_without_events() {
    run_test_sequentially(|store| async move {
        const QUERY: &str = "subscription {
            musicians(orderBy: id, first: 2) {
              name
            }
          }";

        // Execute the subscription and expect at least one result to be
        // available in the result stream
        let stream = run_subscription(&store, QUERY, None).await.unwrap();
        let results: Vec<_> = stream
            .take(1)
            .collect()
            .timeout(Duration::from_secs(3))
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        let result = Arc::try_unwrap(results.into_iter().next().unwrap()).unwrap();
        let data = extract_data!(result).unwrap();
        let exp = object! {
            musicians: vec![
                object! { name: "John" },
                object! { name: "Lisa" }
            ]
        };
        assert_eq!(data, exp);
    })
}

#[test]
fn can_use_nested_filter() {
    const QUERY: &str = "
    query {
        musicians(orderBy: id) {
            name
            bands(where: { originalSongs: [@S1@, @S3@, @S4@] }) { id }
        }
    }
    ";

    run_query(QUERY, |result, _| {
        let exp = object! {
            musicians: vec![
                object! {
                    name: "John",
                    bands: vec![ object! { id: "b2" }]
                },
                object! {
                    name: "Lisa",
                    bands: Vec::<r::Value>::new(),
                },
                object! {
                    name: "Tom",
                    bands: vec![ object! { id: "b2" }]
                },
                object! {
                    name: "Valerie",
                    bands: Vec::<r::Value>::new(),
                }
            ]
        };

        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

// see: graphql-bug-compat
#[test]
fn ignores_invalid_field_arguments() {
    // This query has to return all the musicians since `id` is not a
    // valid argument for the `musicians` field and must therefore be
    // ignored
    const QUERY: &str = "query { musicians(id: \"m1\") { id } } ";

    run_query(QUERY, |result, _| {
        match &result.to_result() {
            // Without validations
            Ok(Some(r::Value::Object(obj))) => match obj.get("musicians").unwrap() {
                r::Value::List(lst) => {
                    assert_eq!(4, lst.len());
                }
                _ => panic!("expected a list of values"),
            },
            // With validations
            Err(e) => {
                match e.get(0).unwrap() {
                    QueryError::ExecutionError(QueryExecutionError::ValidationError(
                        _pos,
                        message,
                    )) => {
                        assert_eq!(
                            message,
                            "Unknown argument \"id\" on field \"Query.musicians\"."
                        );
                    }
                    r => panic!("unexpexted query error: {:?}", r),
                };
            }
            r => {
                panic!("unexpexted result: {:?}", r);
            }
        }
    })
}

// see: graphql-bug-compat
#[test]
fn leaf_selection_mismatch() {
    const QUERY1: &str = "query { musician(id: \"m1\") { id name { wat }} } ";

    run_query(QUERY1, |result, _| {
        let exp = object! { musician: object! { id: "m1", name: "John" } };

        match &result.to_result() {
            // Without validations
            Ok(Some(data)) => {
                assert_eq!(exp, *data);
            }
            // With validations
            Err(e) => {
                match e.get(0).unwrap() {
                    QueryError::ExecutionError(QueryExecutionError::ValidationError(
                        _pos,
                        message,
                    )) => {
                        assert_eq!(message, "Field \"name\" must not have a selection since type \"String!\" has no subfields.");
                    }
                    r => panic!("unexpexted query error: {:?}", r),
                };
                match e.get(1).unwrap() {
                    QueryError::ExecutionError(QueryExecutionError::ValidationError(
                        _pos,
                        message,
                    )) => {
                        assert_eq!(message, "Cannot query field \"wat\" on type \"String\".");
                    }
                    r => panic!("unexpexted query error: {:?}", r),
                }
            }
            r => {
                panic!("unexpexted result: {:?}", r);
            }
        }
    });

    const QUERY2: &str = "query { musician(id: \"m1\") { id name mainBand } } ";
    run_query(QUERY2, |result, _| {
        let exp = object! { musician: object! { id: "m1", name: "John" } };

        match &result.to_result() {
            // Without validations
            Ok(Some(data)) => {
                assert_eq!(exp, *data);
            }
            // With validations
            Err(e) => {
                match e.get(0).unwrap() {
                    QueryError::ExecutionError(QueryExecutionError::ValidationError(
                        _pos,
                        message,
                    )) => {
                        assert_eq!(message, "Field \"mainBand\" of type \"Band\" must have a selection of subfields. Did you mean \"mainBand { ... }\"?");
                    }
                    r => panic!("unexpexted query error: {:?}", r),
                };
            }
            r => {
                panic!("unexpexted result: {:?}", r);
            }
        }
    })
}

// see: graphql-bug-compat
#[test]
fn missing_variable() {
    // '$first' is not defined, use its default from the schema
    const QUERY1: &str = "query { musicians(first: $first) { id } }";
    run_query(QUERY1, |result, _| {
        let exp = object! {
            musicians: vec![
                object! { id: "m1" },
                object! { id: "m2" },
                object! { id: "m3" },
                object! { id: "m4" },
            ]
        };

        match &result.to_result() {
            // We silently set `$first` to 100 and `$skip` to 0, and therefore
            Ok(Some(data)) => {
                assert_eq!(exp, *data);
            }
            // With GraphQL validations active, this query fails
            Err(e) => match e.get(0).unwrap() {
                QueryError::ExecutionError(QueryExecutionError::ValidationError(_pos, message)) => {
                    assert_eq!(message, "Variable \"$first\" is not defined.");
                }
                r => panic!("unexpexted query error: {:?}", r),
            },
            r => {
                panic!("unexpexted result: {:?}", r);
            }
        }
    });

    // '$where' is not defined but nullable, ignore the argument
    const QUERY2: &str = "query { musicians(where: $where) { id } }";
    run_query(QUERY2, |result, _| {
        let exp = object! {
            musicians: vec![
                object! { id: "m1" },
                object! { id: "m2" },
                object! { id: "m3" },
                object! { id: "m4" },
            ]
        };

        match &result.to_result() {
            // '$where' is not defined but nullable, ignore the argument
            Ok(Some(data)) => {
                assert_eq!(exp, *data);
            }
            // With GraphQL validations active, this query fails
            Err(e) => match e.get(0).unwrap() {
                QueryError::ExecutionError(QueryExecutionError::ValidationError(_pos, message)) => {
                    assert_eq!(message, "Variable \"$where\" is not defined.");
                }
                r => panic!("unexpexted query error: {:?}", r),
            },
            r => {
                panic!("unexpexted result: {:?}", r);
            }
        }
    })
}

// see: graphql-bug-compat
// Test that queries with nonmergeable fields do not cause a panic. Can be
// deleted once queries are validated
#[test]
fn invalid_field_merge() {
    const QUERY: &str = "query { musicians { t: id t: mainBand { id } } }";

    run_query(QUERY, |result, _| {
        assert!(result.has_errors());
    })
}

/// What we expect the query to return: either a list of musician ids when
/// the query should succeed (`Ok`) or a string that should appear in the
/// error message when the query should return an `Err`. The error string
/// can contain `@DEPLOYMENT@` which will be replaced with the deployment id
type Expected = Result<Vec<&'static str>, &'static str>;

fn check_musicians_at(query0: &str, block_var: r::Value, expected: Expected, qid: &'static str) {
    run_query((query0, block_var), move |result, id_type| {
        match &expected {
            Ok(ids) => {
                let ids: Vec<_> = ids.iter().map(|id| object! { id: *id }).collect();
                let expected = Some(object_value(vec![("musicians", r::Value::List(ids))]));
                let data = match result.to_result() {
                    Err(errors) => panic!("unexpected error: {:?} ({})\n", errors, qid),
                    Ok(data) => data,
                };
                assert_eq!(data, expected, "failed query: ({})", qid);
            }
            Err(msg) => {
                let errors = match result.to_result() {
                    Err(errors) => errors,
                    Ok(_) => panic!(
                        "expected error `{}` but got successful result ({})",
                        msg, qid
                    ),
                };
                let actual = errors
                    .first()
                    .expect("we expect one error message")
                    .to_string();
                let msg = msg.replace("@DEPLOYMENT@", id_type.deployment_id());
                assert!(
                    actual.contains(&msg),
                    "expected error message `{}` but got {:?} ({})",
                    msg,
                    errors,
                    qid
                );
            }
        };
    });
}

#[test]
fn query_at_block() {
    use test_store::block_store::{FakeBlock, BLOCK_ONE, BLOCK_THREE, BLOCK_TWO, GENESIS_BLOCK};

    fn musicians_at(block: &str, expected: Expected, qid: &'static str) {
        let query = format!("query {{ musicians(block: {{ {} }}) {{ id }} }}", block);
        check_musicians_at(&query, object! {}, expected, qid);
    }

    fn hash(block: &FakeBlock) -> String {
        format!("hash : \"0x{}\"", block.hash)
    }

    const BLOCK_NOT_INDEXED: &str = "subgraph @DEPLOYMENT@ has only indexed \
         up to block number 1 and data for block number 7000 is therefore not yet available";
    const BLOCK_NOT_INDEXED2: &str = "subgraph @DEPLOYMENT@ has only indexed \
         up to block number 1 and data for block number 2 is therefore not yet available";
    const BLOCK_HASH_NOT_FOUND: &str = "no block with that hash found";

    musicians_at("number: 7000", Err(BLOCK_NOT_INDEXED), "n7000");
    musicians_at("number: 0", Ok(vec!["m1", "m2"]), "n0");
    musicians_at("number: 1", Ok(vec!["m1", "m2", "m3", "m4"]), "n1");

    musicians_at(&hash(&GENESIS_BLOCK), Ok(vec!["m1", "m2"]), "h0");
    musicians_at(&hash(&BLOCK_ONE), Ok(vec!["m1", "m2", "m3", "m4"]), "h1");
    musicians_at(&hash(&BLOCK_TWO), Err(BLOCK_NOT_INDEXED2), "h2");
    musicians_at(&hash(&BLOCK_THREE), Err(BLOCK_HASH_NOT_FOUND), "h3");
}

#[test]
fn query_at_block_with_vars() {
    use test_store::block_store::{FakeBlock, BLOCK_ONE, BLOCK_THREE, BLOCK_TWO, GENESIS_BLOCK};

    fn musicians_at_nr(block: i32, expected: Expected, qid: &'static str) {
        let query = "query by_nr($block: Int!) { musicians(block: { number: $block }) { id } }";
        let var = object! { block: block };

        check_musicians_at(query, var, expected.clone(), qid);

        let query = "query by_nr($block: Block_height!) { musicians(block: $block) { id } }";
        let var = object! { block: object! { number: block } };

        check_musicians_at(query, var, expected, qid);
    }

    fn musicians_at_nr_gte(block: i32, expected: Expected, qid: &'static str) {
        let query = "query by_nr($block: Int!) { musicians(block: { number_gte: $block }) { id } }";
        let var = object! { block: block };

        check_musicians_at(query, var, expected, qid);
    }

    fn musicians_at_hash(block: &FakeBlock, expected: Expected, qid: &'static str) {
        let query = "query by_hash($block: Bytes!) { musicians(block: { hash: $block }) { id } }";
        let var = object! { block: block.hash.to_string() };

        check_musicians_at(query, var, expected, qid);
    }

    const BLOCK_NOT_INDEXED: &str = "subgraph @DEPLOYMENT@ has only indexed \
         up to block number 1 and data for block number 7000 is therefore not yet available";
    const BLOCK_NOT_INDEXED2: &str = "subgraph @DEPLOYMENT@ has only indexed \
         up to block number 1 and data for block number 2 is therefore not yet available";
    const BLOCK_HASH_NOT_FOUND: &str = "no block with that hash found";

    musicians_at_nr(7000, Err(BLOCK_NOT_INDEXED), "n7000");
    musicians_at_nr(0, Ok(vec!["m1", "m2"]), "n0");
    musicians_at_nr(1, Ok(vec!["m1", "m2", "m3", "m4"]), "n1");

    musicians_at_nr_gte(7000, Err(BLOCK_NOT_INDEXED), "ngte7000");
    musicians_at_nr_gte(0, Ok(vec!["m1", "m2", "m3", "m4"]), "ngte0");
    musicians_at_nr_gte(1, Ok(vec!["m1", "m2", "m3", "m4"]), "ngte1");

    musicians_at_hash(&GENESIS_BLOCK, Ok(vec!["m1", "m2"]), "h0");
    musicians_at_hash(&BLOCK_ONE, Ok(vec!["m1", "m2", "m3", "m4"]), "h1");
    musicians_at_hash(&BLOCK_TWO, Err(BLOCK_NOT_INDEXED2), "h2");
    musicians_at_hash(&BLOCK_THREE, Err(BLOCK_HASH_NOT_FOUND), "h3");
}

#[test]
fn query_detects_reorg() {
    async fn query_at(deployment: &DeploymentLocator, block: i32) -> QueryResult {
        let query =
            format!("query {{ musician(id: \"m1\", block: {{ number: {block} }}) {{ id }} }}");
        execute_query(deployment, &query).await
    }

    run_test_sequentially(|store| async move {
        let deployment = setup(
            store.as_ref(),
            "graphqlQueryDetectsReorg",
            BTreeSet::new(),
            IdType::String,
        )
        .await;
        // Initial state with latest block at block 1
        let state = deployment_state(STORE.as_ref(), &deployment.hash).await;

        // Inject a fake initial state; c435c25decbc4ad7bbbadf8e0ced0ff2
        *graph_graphql::test_support::INITIAL_DEPLOYMENT_STATE_FOR_TESTS
            .lock()
            .unwrap() = Some(state);

        // When there is no revert, queries work fine
        let result = query_at(&deployment, 1).await;

        assert_eq!(
            extract_data!(result),
            Some(object!(musician: object!(id: "m1")))
        );

        // Revert one block
        revert_block(&STORE, &deployment, &GENESIS_PTR).await;

        // A query is still fine since we query at block 0; we were at block
        // 1 when we got `state`, and reorged once by one block, which can
        // not affect block 0, and it's therefore ok to query at block 0
        // even with a concurrent reorg
        let result = query_at(&deployment, 0).await;
        assert_eq!(
            extract_data!(result),
            Some(object!(musician: object!(id: "m1")))
        );

        // We move the subgraph head forward. The state we have is also for
        // block 1, but with a smaller reorg count and we therefore report
        // an error
        test_store::transact_and_wait(
            &STORE.subgraph_store(),
            &deployment,
            BLOCK_ONE.clone(),
            vec![],
        )
        .await
        .unwrap();

        let result = query_at(&deployment, 1).await;
        match result.to_result().unwrap_err()[0] {
            QueryError::ExecutionError(QueryExecutionError::DeploymentReverted) => { /* expected */
            }
            _ => panic!("unexpected error from block reorg"),
        }

        // Reset the fake initial state; c435c25decbc4ad7bbbadf8e0ced0ff2
        *graph_graphql::test_support::INITIAL_DEPLOYMENT_STATE_FOR_TESTS
            .lock()
            .unwrap() = None;
    })
}

#[test]
fn can_query_meta() {
    // metadata for the latest block (block 1)
    const QUERY1: &str =
        "query { _meta { deployment block { hash number __typename } __typename } }";
    run_query(QUERY1, |result, id_type| {
        let exp = object! {
            _meta: object! {
                deployment: id_type.deployment_id(),
                block: object! {
                    hash: "0x8511fa04b64657581e3f00e14543c1d522d5d7e771b54aa3060b662ade47da13",
                    number: 1,
                    __typename: "_Block_"
                },
                __typename: "_Meta_"
            },
        };
        assert_eq!(extract_data!(result), Some(exp));
    });

    // metadata for block 0 by number
    const QUERY2: &str =
        "query { _meta(block: { number: 0 }) { deployment block { hash number } } }";
    run_query(QUERY2, |result, id_type| {
        let exp = object! {
            _meta: object! {
                deployment: id_type.deployment_id(),
                block: object! {
                    hash: r::Value::Null,
                    number: 0
                },
            },
        };
        assert_eq!(extract_data!(result), Some(exp));
    });

    // metadata for block 0 by hash
    const QUERY3: &str = "query { _meta(block: { hash: \"bd34884280958002c51d3f7b5f853e6febeba33de0f40d15b0363006533c924f\" }) { \
                                        deployment block { hash number } } }";
    run_query(QUERY3, |result, id_type| {
        let exp = object! {
            _meta: object! {
                deployment: id_type.deployment_id(),
                block: object! {
                    hash: "0xbd34884280958002c51d3f7b5f853e6febeba33de0f40d15b0363006533c924f",
                    number: 0
                },
            },
        };
        assert_eq!(extract_data!(result), Some(exp));
    });

    // metadata for block 2, which is beyond what the subgraph has indexed
    const QUERY4: &str =
        "query { _meta(block: { number: 2 }) { deployment block { hash number } } }";
    run_query(QUERY4, |result, _| {
        assert!(result.has_errors());
    });
}

#[test]
fn non_fatal_errors() {
    use serde_json::json;
    use test_store::block_store::BLOCK_TWO;

    run_test_sequentially(|store| async move {
        let deployment = setup(
            store.as_ref(),
            "testNonFatalErrors",
            BTreeSet::from_iter(Some(SubgraphFeature::NonFatalErrors)),
            IdType::String,
        )
        .await;

        let err = SubgraphError {
            subgraph_id: deployment.hash.clone(),
            message: "cow template handler could not moo event transaction".to_string(),
            block_ptr: Some(BLOCK_TWO.block_ptr()),
            handler: Some("handleMoo".to_string()),
            deterministic: true,
        };

        transact_errors(&STORE, &deployment, BLOCK_TWO.block_ptr(), vec![err], true)
            .await
            .unwrap();

        // `subgraphError` is implicitly `deny`, data is omitted.
        let query = "query { musician(id: \"m1\") { id } }";
        let result = execute_query(&deployment, query).await;
        let expected = json!({
            "errors": [
                {
                    "message": "indexing_error"
                }
            ]
        });
        assert_eq!(expected, serde_json::to_value(&result).unwrap());

        // Same result for explicit `deny`.
        let query = "query { musician(id: \"m1\", subgraphError: deny) { id } }";
        let result = execute_query(&deployment, query).await;
        assert_eq!(expected, serde_json::to_value(&result).unwrap());

        // But `_meta` is still returned.
        let query = "query { musician(id: \"m1\") { id }  _meta { hasIndexingErrors } }";
        let result = execute_query(&deployment, query).await;
        let expected = json!({
            "data": {
                "_meta": {
                    "hasIndexingErrors": true
                }
            },
            "errors": [
                {
                    "message": "indexing_error"
                }
            ]
        });
        assert_eq!(expected, serde_json::to_value(&result).unwrap());

        // Introspection queries are not affected.
        let query =
            "query { __schema { queryType { name } } __type(name: \"Musician\") { name }  }";
        let result = execute_query(&deployment, query).await;
        let expected = json!({
            "data": {
                "__schema": {
                    "queryType": {
                        "name": "Query"
                    }
                },
                "__type": {
                    "name": "Musician"
                }
            },
            "errors": [
                {
                    "message": "indexing_error"
                }
            ]
        });
        assert_eq!(expected, serde_json::to_value(&result).unwrap());

        let query = "query { __type(name: \"Musician\") { name } }";
        let result = execute_query(&deployment, query).await;
        let expected = json!({
            "data": {
                "__type": {
                    "name": "Musician"
                }
            },
            "errors": [
                {
                    "message": "indexing_error"
                }
            ]
        });
        assert_eq!(expected, serde_json::to_value(&result).unwrap());

        // With `allow`, the error remains but the data is included.
        let query = "query { musician(id: \"m1\", subgraphError: allow) { id } }";
        let result = execute_query(&deployment, query).await;
        let expected = json!({
            "data": {
                "musician": {
                    "id": "m1"
                }
            },
            "errors": [
                {
                    "message": "indexing_error"
                }
            ]
        });
        assert_eq!(expected, serde_json::to_value(&result).unwrap());

        // Test error reverts.
        revert_block(&STORE, &deployment, &BLOCK_ONE).await;
        let query = "query { musician(id: \"m1\") { id }  _meta { hasIndexingErrors } }";
        let result = execute_query(&deployment, query).await;
        let expected = json!({
            "data": {
                "musician": {
                    "id": "m1"
                },
                "_meta": {
                    "hasIndexingErrors": false
                }
            }
        });
        assert_eq!(expected, serde_json::to_value(&result).unwrap());
    })
}

#[test]
fn can_query_root_typename() {
    const QUERY: &str = "query { __typename }";
    run_query(QUERY, |result, _| {
        let exp = object! {
            __typename: "Query"
        };
        assert_eq!(extract_data!(result), Some(exp));
    })
}

#[test]
fn deterministic_error() {
    use serde_json::json;
    use test_store::block_store::BLOCK_TWO;

    run_test_sequentially(|store| async move {
        let deployment = setup(
            store.as_ref(),
            "testDeterministicError",
            BTreeSet::new(),
            IdType::String,
        )
        .await;

        let err = SubgraphError {
            subgraph_id: deployment.hash.clone(),
            message: "cow template handler could not moo event transaction".to_string(),
            block_ptr: Some(BLOCK_TWO.block_ptr()),
            handler: Some("handleMoo".to_string()),
            deterministic: true,
        };

        transact_errors(&STORE, &deployment, BLOCK_TWO.block_ptr(), vec![err], false)
            .await
            .unwrap();

        // `subgraphError` is implicitly `deny`, data is omitted.
        let query = "query { musician(id: \"m1\") { id } }";
        let result = execute_query(&deployment, query).await;
        let expected = json!({
            "errors": [
                {
                    "message": "indexing_error"
                }
            ]
        });
        assert_eq!(expected, serde_json::to_value(&result).unwrap());

        // Introspection queries are not affected.
        let query = "query { __schema { queryType { name } } }";
        let result = execute_query(&deployment, query).await;
        let expected = json!({
            "data": {
                "__schema": {
                    "queryType": {
                        "name": "Query"
                    }
                }
            },
            "errors": [
                {
                    "message": "indexing_error"
                }
            ]
        });
        assert_eq!(expected, serde_json::to_value(&result).unwrap());
    })
}

#[test]
fn can_query_with_or_filter() {
    const QUERY: &str = "
    query {
        musicians(where: { or: [{ name: \"John\" }, { id: \"m2\" }] }) {
            name
            id
        }
    }
    ";

    run_query(QUERY, |result, _| {
        let exp = object! {
            musicians: vec![
                object! { name: "John", id: "m1" },
                object! { name: "Lisa", id: "m2" },
            ],
        };
        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_with_or_filter_fields_always_and() {
    const QUERY: &str = "
    query {
        musicians(where: { or: [{ name: \"John\", id: \"m2\" }] }) {
            name
            id
        }
    }
    ";

    run_query(QUERY, |result, _| {
        let exp = object! {
            musicians: r::Value::List(vec![]),
        };
        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_with_and_filter() {
    const QUERY: &str = "
    query {
        musicians(where: { and: [{ name: \"John\", id: \"m2\" }] }) {
          name
          id
        }
      }
    ";

    run_query(QUERY, |result, _| {
        let exp = object! {
            musicians: r::Value::List(vec![]),
        };
        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_with_or_and_filter() {
    const QUERY: &str = "
    query {
        musicians(
            where: { or: [{ name: \"John\", id: \"m1\" }, { mainBand: \"b2\" }] }
        ) {
          name
          id
        }
      }
    ";

    run_query(QUERY, |result, _| {
        let exp = object! {
            musicians: vec![
                object! { name: "John", id: "m1" },
                object! { name: "Tom", id: "m3" },
            ],
        };
        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_with_or_explicit_and_filter() {
    const QUERY: &str = "
    query {
        musicians(
          where: { or: [{ and: [{ name: \"John\", id: \"m1\" }] }, { mainBand: \"b2\" }] }
        ) {
          name
          id
        }
      }
    ";

    run_query(QUERY, |result, _| {
        let exp = object! {
            musicians: vec![
                object! { name: "John", id: "m1" },
                object! { name: "Tom", id: "m3" },
            ],
        };
        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn can_query_with_or_implicit_and_filter() {
    const QUERY: &str = "
    query {
        musicians(
          where: { or: [{ name: \"John\", id: \"m1\" }, { name: \"Lisa\", id: \"m2\" }] }
        ) {
          name
          id
        }
      }
    ";

    run_query(QUERY, |result, _| {
        let exp = object! {
            musicians: vec![
                object! { name: "John", id: "m1" },
                object! { name: "Lisa", id: "m2" },
            ],
        };
        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}

#[test]
fn trace_works() {
    run_test_sequentially(|store| async move {
        let deployment = setup_readonly(store.as_ref()).await;
        let query = Query::new(
            graphql_parser::parse_query("query { musicians(first: 100) { name } }")
                .unwrap()
                .into_static(),
            None,
            true,
        );

        let result = execute_subgraph_query(
            query,
            QueryTarget::Deployment(deployment.hash, Default::default()),
        )
        .await;

        let trace = &result.first().unwrap().trace;
        assert!(!trace.is_none(), "result has a trace");
    })
}

/// Check that various comparisons against `id` work as expected. This also
/// serves as a test that they work for `String` as well as `Bytes` fields
/// in general
#[test]
fn can_compare_id() {
    // For each entry `(cond, sids)` in this array, check that a query with
    // a where clause `cond` returns a list of songs whose `sid` are the
    // ones listed in `sids`
    let checks = [
        ("id_gt:  @S2@", vec!["s3", "s4"]),
        ("id_gte: @S2@", vec!["s2", "s3", "s4"]),
        ("id_lt:  @S2@", vec!["s1"]),
        ("id_lte: @S2@", vec!["s1", "s2"]),
        ("id_not: @S2@", vec!["s1", "s3", "s4"]),
    ];

    for (cond, sids) in checks {
        let query = format!("query {{ songs(where: {{ {cond} }}) {{ sid }} }}");
        let sids: Vec<_> = sids
            .iter()
            .map(|sid| object! { sid: sid.to_string() })
            .collect();
        let exp = object! { songs: sids };
        run_query(query, move |result, id_type| {
            let data = extract_data!(result).unwrap();
            assert_eq!(data, exp, "check {} for {:?} ids", cond, id_type);
        })
    }
}

#[test]
fn empty_type_c() {
    // Single `rl4` has no songs. Make sure our SQL query generation does
    // not cause a syntax error
    const QUERY: &str = "
    query {
        single(id: \"rl4\") {
            songs { id }
        }
    }";

    run_query(QUERY, |result, _| {
        let exp = object! {
            single: object! { songs: Vec::<r::Value>::new() }
        };
        let data = extract_data!(result).unwrap();
        assert_eq!(data, exp);
    })
}
