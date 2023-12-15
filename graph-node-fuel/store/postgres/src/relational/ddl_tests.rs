use itertools::Itertools;
use pretty_assertions::assert_eq;

use super::*;

use crate::layout_for_tests::make_dummy_site;

const ID_TYPE: ColumnType = ColumnType::String;

fn test_layout(gql: &str) -> Layout {
    let subgraph = DeploymentHash::new("subgraph").unwrap();
    let schema = InputSchema::parse(gql, subgraph.clone()).expect("Test schema invalid");
    let namespace = Namespace::new("sgd0815".to_owned()).unwrap();
    let site = Arc::new(make_dummy_site(subgraph, namespace, "anet".to_string()));
    let ents = {
        match schema.entity_type("FileThing") {
            Ok(entity_type) => BTreeSet::from_iter(vec![entity_type]),
            Err(_) => BTreeSet::new(),
        }
    };
    let catalog = Catalog::for_tests(site.clone(), ents).expect("Can not create catalog");
    Layout::new(site, &schema, catalog).expect("Failed to construct Layout")
}

#[test]
fn table_is_sane() {
    let layout = test_layout(THING_GQL);
    let table = layout
        .table(&"thing".into())
        .expect("failed to get 'thing' table");
    assert_eq!(SqlName::from("thing"), table.name);
    assert_eq!("Thing", table.object.as_str());

    let id = table
        .column(&PRIMARY_KEY_COLUMN.into())
        .expect("failed to get 'id' column for 'thing' table");
    assert_eq!(ID_TYPE, id.column_type);
    assert!(!id.is_nullable());
    assert!(!id.is_list());

    let big_thing = table
        .column(&"big_thing".into())
        .expect("failed to get 'big_thing' column for 'thing' table");
    assert_eq!(ID_TYPE, big_thing.column_type);
    assert!(!big_thing.is_nullable());
    // Field lookup happens by the SQL name, not the GraphQL name
    let bad_sql_name = SqlName("bigThing".to_owned());
    assert!(table.column(&bad_sql_name).is_none());
}

// Check that the two strings are the same after replacing runs of
// whitespace with a single space
#[track_caller]
fn check_eqv(left: &str, right: &str) {
    let left_s = left.split_whitespace().join(" ");
    let right_s = right.split_whitespace().join(" ");
    if left_s != right_s {
        // Make sure the original strings show up in the error message
        assert_eq!(left, right);
    }
}

#[test]
fn generate_ddl() {
    let layout = test_layout(THING_GQL);
    let sql = layout.as_ddl().expect("Failed to generate DDL");
    assert_eq!(THING_DDL, &sql); // Use `assert_eq!` to also test the formatting.

    let layout = test_layout(MUSIC_GQL);
    let sql = layout.as_ddl().expect("Failed to generate DDL");
    check_eqv(MUSIC_DDL, &sql);

    let layout = test_layout(FOREST_GQL);
    let sql = layout.as_ddl().expect("Failed to generate DDL");
    check_eqv(FOREST_DDL, &sql);

    let layout = test_layout(FULLTEXT_GQL);
    let sql = layout.as_ddl().expect("Failed to generate DDL");
    check_eqv(FULLTEXT_DDL, &sql);

    let layout = test_layout(FORWARD_ENUM_GQL);
    let sql = layout.as_ddl().expect("Failed to generate DDL");
    check_eqv(FORWARD_ENUM_SQL, &sql);
}

#[test]
fn exlusion_ddl() {
    let layout = test_layout(THING_GQL);
    let table = layout
        .table_for_entity(&layout.input_schema.entity_type("Thing").unwrap())
        .unwrap();

    // When `as_constraint` is false, just create an index
    let mut out = String::new();
    table
        .exclusion_ddl_inner(&mut out, false)
        .expect("can write exclusion DDL");
    check_eqv(
        r#"create index thing_id_block_range_excl on "sgd0815"."thing" using gist (id, block_range);"#,
        out.trim(),
    );

    // When `as_constraint` is true, add an exclusion constraint
    let mut out = String::new();
    table
        .exclusion_ddl_inner(&mut out, true)
        .expect("can write exclusion DDL");
    check_eqv(
        r#"alter table "sgd0815"."thing" add constraint thing_id_block_range_excl exclude using gist (id with =, block_range with &&);"#,
        out.trim(),
    );
}

#[test]
fn forward_enum() {
    let layout = test_layout(FORWARD_ENUM_GQL);
    let table = layout
        .table(&SqlName::from("thing"))
        .expect("thing table exists");
    let column = table
        .column(&SqlName::from("orientation"))
        .expect("orientation column exists");
    assert!(column.is_enum());
}

#[test]
fn can_copy_from() {
    let source = test_layout(THING_GQL);
    // We can always copy from an identical layout
    assert!(source.can_copy_from(&source).is_empty());

    // We allow leaving out and adding types, and leaving out attributes
    // of existing types
    let dest =
        test_layout("type Scalar @entity { id: ID } type Other @entity { id: ID, int: Int! }");
    assert!(dest.can_copy_from(&source).is_empty());

    // We allow making a non-nullable attribute nullable
    let dest = test_layout("type Thing @entity { id: ID! }");
    assert!(dest.can_copy_from(&source).is_empty());

    // We can not turn a non-nullable attribute into a nullable attribute
    let dest = test_layout("type Scalar @entity { id: ID! }");
    assert_eq!(
        vec![
            "The attribute Scalar.id is non-nullable, but the \
                 corresponding attribute in the source is nullable"
        ],
        dest.can_copy_from(&source)
    );

    // We can not change a scalar field to an array
    let dest = test_layout("type Scalar @entity { id: ID, string: [String] }");
    assert_eq!(
        vec![
            "The attribute Scalar.string has type [String], \
                 but its type in the source is String"
        ],
        dest.can_copy_from(&source)
    );
    // We can not change an array field to a scalar
    assert_eq!(
        vec![
            "The attribute Scalar.string has type String, \
                 but its type in the source is [String]"
        ],
        source.can_copy_from(&dest)
    );
    // We can not change the underlying type of a field
    let dest = test_layout("type Scalar @entity { id: ID, color: Int }");
    assert_eq!(
        vec![
            "The attribute Scalar.color has type Int, but \
                 its type in the source is Color"
        ],
        dest.can_copy_from(&source)
    );
    // We can not change the underlying type of a field in arrays
    let source = test_layout("type Scalar @entity { id: ID, color: [Int!]! }");
    let dest = test_layout("type Scalar @entity { id: ID, color: [String!]! }");
    assert_eq!(
        vec![
            "The attribute Scalar.color has type [String!]!, but \
                 its type in the source is [Int!]!"
        ],
        dest.can_copy_from(&source)
    );
}

const THING_GQL: &str = r#"
        type Thing @entity {
            id: ID!
            bigThing: Thing!
        }

        enum Color { yellow, red, BLUE }

        enum Size { small, medium, large }

        type Scalar @entity {
            id: ID,
            bool: Boolean,
            int: Int,
            bigDecimal: BigDecimal,
            string: String,
            bytes: Bytes,
            bigInt: BigInt,
            color: Color,
        }

        type FileThing @entity {
            id: ID!
        }
        "#;

const THING_DDL: &str = r#"create type sgd0815."color"
    as enum ('BLUE', 'red', 'yellow');
create type sgd0815."size"
    as enum ('large', 'medium', 'small');

    create table "sgd0815"."thing" (
        vid                  bigserial primary key,
        block_range          int4range not null,
        "id"                 text not null,
        "big_thing"          text not null
    );

    alter table "sgd0815"."thing"
        add constraint thing_id_block_range_excl exclude using gist (id with =, block_range with &&);
create index brin_thing
    on "sgd0815"."thing"
 using brin(lower(block_range) int4_minmax_ops, coalesce(upper(block_range), 2147483647) int4_minmax_ops, vid int8_minmax_ops);
create index thing_block_range_closed
    on "sgd0815"."thing"(coalesce(upper(block_range), 2147483647))
 where coalesce(upper(block_range), 2147483647) < 2147483647;
create index attr_0_0_thing_id
    on "sgd0815"."thing" using btree("id");
create index attr_0_1_thing_big_thing
    on "sgd0815"."thing" using gist("big_thing", block_range);


    create table "sgd0815"."scalar" (
        vid                  bigserial primary key,
        block_range          int4range not null,
        "id"                 text not null,
        "bool"               boolean,
        "int"                integer,
        "big_decimal"        numeric,
        "string"             text,
        "bytes"              bytea,
        "big_int"            numeric,
        "color"              "sgd0815"."color"
    );

    alter table "sgd0815"."scalar"
        add constraint scalar_id_block_range_excl exclude using gist (id with =, block_range with &&);
create index brin_scalar
    on "sgd0815"."scalar"
 using brin(lower(block_range) int4_minmax_ops, coalesce(upper(block_range), 2147483647) int4_minmax_ops, vid int8_minmax_ops);
create index scalar_block_range_closed
    on "sgd0815"."scalar"(coalesce(upper(block_range), 2147483647))
 where coalesce(upper(block_range), 2147483647) < 2147483647;
create index attr_1_0_scalar_id
    on "sgd0815"."scalar" using btree("id");
create index attr_1_1_scalar_bool
    on "sgd0815"."scalar" using btree("bool");
create index attr_1_2_scalar_int
    on "sgd0815"."scalar" using btree("int");
create index attr_1_3_scalar_big_decimal
    on "sgd0815"."scalar" using btree("big_decimal");
create index attr_1_4_scalar_string
    on "sgd0815"."scalar" using btree(left("string", 256));
create index attr_1_5_scalar_bytes
    on "sgd0815"."scalar" using btree(substring("bytes", 1, 64));
create index attr_1_6_scalar_big_int
    on "sgd0815"."scalar" using btree("big_int");
create index attr_1_7_scalar_color
    on "sgd0815"."scalar" using btree("color");


    create table "sgd0815"."file_thing" (
        vid                  bigserial primary key,
        block_range          int4range not null,
        causality_region     int not null,
        "id"                 text not null
    );

    alter table "sgd0815"."file_thing"
        add constraint file_thing_id_block_range_excl exclude using gist (id with =, block_range with &&);
create index brin_file_thing
    on "sgd0815"."file_thing"
 using brin(lower(block_range) int4_minmax_ops, coalesce(upper(block_range), 2147483647) int4_minmax_ops, vid int8_minmax_ops);
create index file_thing_block_range_closed
    on "sgd0815"."file_thing"(coalesce(upper(block_range), 2147483647))
 where coalesce(upper(block_range), 2147483647) < 2147483647;
create index attr_2_0_file_thing_id
    on "sgd0815"."file_thing" using btree("id");

"#;

const MUSIC_GQL: &str = r#"type Musician @entity {
    id: ID!
    name: String!
    mainBand: Band
    bands: [Band!]!
    writtenSongs: [Song]! @derivedFrom(field: "writtenBy")
}

type Band @entity {
    id: ID!
    name: String!
    members: [Musician!]! @derivedFrom(field: "bands")
    originalSongs: [Song!]!
}

type Song @entity(immutable: true) {
    id: ID!
    title: String!
    writtenBy: Musician!
    band: Band @derivedFrom(field: "originalSongs")
}

type SongStat @entity {
    id: ID!
    song: Song @derivedFrom(field: "id")
    played: Int!
}"#;
const MUSIC_DDL: &str = r#"create table "sgd0815"."musician" (
        vid                  bigserial primary key,
        block_range          int4range not null,
        "id"                 text not null,
        "name"               text not null,
        "main_band"          text,
        "bands"              text[] not null
);
alter table "sgd0815"."musician"
  add constraint musician_id_block_range_excl exclude using gist (id with =, block_range with &&);
create index brin_musician
    on "sgd0815"."musician"
 using brin(lower(block_range) int4_minmax_ops, coalesce(upper(block_range), 2147483647) int4_minmax_ops, vid int8_minmax_ops);
create index musician_block_range_closed
    on "sgd0815"."musician"(coalesce(upper(block_range), 2147483647))
 where coalesce(upper(block_range), 2147483647) < 2147483647;
create index attr_0_0_musician_id
    on "sgd0815"."musician" using btree("id");
create index attr_0_1_musician_name
    on "sgd0815"."musician" using btree(left("name", 256));
create index attr_0_2_musician_main_band
    on "sgd0815"."musician" using gist("main_band", block_range);

create table "sgd0815"."band" (
        vid                  bigserial primary key,
        block_range          int4range not null,
        "id"                 text not null,
        "name"               text not null,
        "original_songs"     text[] not null
);
alter table "sgd0815"."band"
  add constraint band_id_block_range_excl exclude using gist (id with =, block_range with &&);
create index brin_band
    on "sgd0815"."band"
 using brin(lower(block_range) int4_minmax_ops, coalesce(upper(block_range), 2147483647) int4_minmax_ops, vid int8_minmax_ops);
create index band_block_range_closed
    on "sgd0815"."band"(coalesce(upper(block_range), 2147483647))
 where coalesce(upper(block_range), 2147483647) < 2147483647;
create index attr_1_0_band_id
    on "sgd0815"."band" using btree("id");
create index attr_1_1_band_name
    on "sgd0815"."band" using btree(left("name", 256));

create table "sgd0815"."song" (
        vid                    bigserial primary key,
        block$                 int not null,
        "id"                 text not null,
        "title"              text not null,
        "written_by"         text not null,

        unique(id)
);
create index brin_song
    on "sgd0815"."song"
 using brin(block$ int4_minmax_ops, vid int8_minmax_ops);
create index attr_2_0_song_title
    on "sgd0815"."song" using btree(left("title", 256));
create index attr_2_1_song_written_by
    on "sgd0815"."song" using btree("written_by", block$);

create table "sgd0815"."song_stat" (
        vid                  bigserial primary key,
        block_range          int4range not null,
        "id"                 text not null,
        "played"             integer not null
);
alter table "sgd0815"."song_stat"
  add constraint song_stat_id_block_range_excl exclude using gist (id with =, block_range with &&);
create index brin_song_stat
    on "sgd0815"."song_stat"
 using brin(lower(block_range) int4_minmax_ops, coalesce(upper(block_range), 2147483647) int4_minmax_ops, vid int8_minmax_ops);
create index song_stat_block_range_closed
    on "sgd0815"."song_stat"(coalesce(upper(block_range), 2147483647))
 where coalesce(upper(block_range), 2147483647) < 2147483647;
create index attr_3_0_song_stat_id
    on "sgd0815"."song_stat" using btree("id");
create index attr_3_1_song_stat_played
    on "sgd0815"."song_stat" using btree("played");

"#;

const FOREST_GQL: &str = r#"
interface ForestDweller {
    id: ID!,
    forest: Forest
}
type Animal implements ForestDweller @entity {
     id: ID!,
     forest: Forest
}
type Forest @entity {
    id: ID!,
    # Array of interfaces as derived reference
    dwellers: [ForestDweller!]! @derivedFrom(field: "forest")
}
type Habitat @entity {
    id: ID!,
    # Use interface as direct reference
    most_common: ForestDweller!,
    dwellers: [ForestDweller!]!
}"#;

const FOREST_DDL: &str = r#"create table "sgd0815"."animal" (
        vid                  bigserial primary key,
        block_range          int4range not null,
        "id"                 text not null,
        "forest"             text
);
alter table "sgd0815"."animal"
  add constraint animal_id_block_range_excl exclude using gist (id with =, block_range with &&);
create index brin_animal
    on "sgd0815"."animal"
 using brin(lower(block_range) int4_minmax_ops, coalesce(upper(block_range), 2147483647) int4_minmax_ops, vid int8_minmax_ops);
create index animal_block_range_closed
    on "sgd0815"."animal"(coalesce(upper(block_range), 2147483647))
 where coalesce(upper(block_range), 2147483647) < 2147483647;
create index attr_0_0_animal_id
    on "sgd0815"."animal" using btree("id");
create index attr_0_1_animal_forest
    on "sgd0815"."animal" using gist("forest", block_range);

create table "sgd0815"."forest" (
        vid                  bigserial primary key,
        block_range          int4range not null,
        "id"               text not null
);
alter table "sgd0815"."forest"
  add constraint forest_id_block_range_excl exclude using gist (id with =, block_range with &&);
create index brin_forest
    on "sgd0815"."forest"
 using brin(lower(block_range) int4_minmax_ops, coalesce(upper(block_range), 2147483647) int4_minmax_ops, vid int8_minmax_ops);
create index forest_block_range_closed
    on "sgd0815"."forest"(coalesce(upper(block_range), 2147483647))
 where coalesce(upper(block_range), 2147483647) < 2147483647;
create index attr_1_0_forest_id
    on "sgd0815"."forest" using btree("id");

create table "sgd0815"."habitat" (
        vid                  bigserial primary key,
        block_range          int4range not null,
        "id"                 text not null,
        "most_common"        text not null,
        "dwellers"           text[] not null
);
alter table "sgd0815"."habitat"
  add constraint habitat_id_block_range_excl exclude using gist (id with =, block_range with &&);
create index brin_habitat
    on "sgd0815"."habitat"
 using brin(lower(block_range) int4_minmax_ops, coalesce(upper(block_range), 2147483647) int4_minmax_ops, vid int8_minmax_ops);
create index habitat_block_range_closed
    on "sgd0815"."habitat"(coalesce(upper(block_range), 2147483647))
 where coalesce(upper(block_range), 2147483647) < 2147483647;
create index attr_2_0_habitat_id
    on "sgd0815"."habitat" using btree("id");
create index attr_2_1_habitat_most_common
    on "sgd0815"."habitat" using gist("most_common", block_range);

"#;
const FULLTEXT_GQL: &str = r#"
type _Schema_ @fulltext(
    name: "search"
    language: en
    algorithm: rank
    include: [
        {
            entity: "Animal",
            fields: [
                {name: "name"},
                {name: "species"}
            ]
        }
    ]
)
type Animal @entity  {
    id: ID!,
    name: String!
    species: String!
    forest: Forest
}
type Forest @entity {
    id: ID!,
    dwellers: [Animal!]! @derivedFrom(field: "forest")
}
type Habitat @entity {
    id: ID!,
    most_common: Animal!,
    dwellers: [Animal!]!
}"#;

const FULLTEXT_DDL: &str = r#"create table "sgd0815"."animal" (
        vid                  bigserial primary key,
        block_range          int4range not null,
        "id"                 text not null,
        "name"               text not null,
        "species"            text not null,
        "forest"             text,
        "search"             tsvector
);
alter table "sgd0815"."animal"
  add constraint animal_id_block_range_excl exclude using gist (id with =, block_range with &&);
create index brin_animal
    on "sgd0815"."animal"
 using brin(lower(block_range) int4_minmax_ops, coalesce(upper(block_range), 2147483647) int4_minmax_ops, vid int8_minmax_ops);
create index animal_block_range_closed
    on "sgd0815"."animal"(coalesce(upper(block_range), 2147483647))
 where coalesce(upper(block_range), 2147483647) < 2147483647;
create index attr_0_0_animal_id
    on "sgd0815"."animal" using btree("id");
create index attr_0_1_animal_name
    on "sgd0815"."animal" using btree(left("name", 256));
create index attr_0_2_animal_species
    on "sgd0815"."animal" using btree(left("species", 256));
create index attr_0_3_animal_forest
    on "sgd0815"."animal" using gist("forest", block_range);
create index attr_0_4_animal_search
    on "sgd0815"."animal" using gin("search");

create table "sgd0815"."forest" (
        vid                  bigserial primary key,
        block_range          int4range not null,
        "id"                 text not null
);
alter table "sgd0815"."forest"
  add constraint forest_id_block_range_excl exclude using gist (id with =, block_range with &&);

create index brin_forest
    on "sgd0815"."forest"
 using brin(lower(block_range) int4_minmax_ops, coalesce(upper(block_range), 2147483647) int4_minmax_ops, vid int8_minmax_ops);
create index forest_block_range_closed
    on "sgd0815"."forest"(coalesce(upper(block_range), 2147483647))
 where coalesce(upper(block_range), 2147483647) < 2147483647;
create index attr_1_0_forest_id
    on "sgd0815"."forest" using btree("id");

create table "sgd0815"."habitat" (
        vid                  bigserial primary key,
        block_range          int4range not null,
        "id"                 text not null,
        "most_common"        text not null,
        "dwellers"           text[] not null
);
alter table "sgd0815"."habitat"
  add constraint habitat_id_block_range_excl exclude using gist (id with =, block_range with &&);
create index brin_habitat
    on "sgd0815"."habitat"
 using brin(lower(block_range) int4_minmax_ops, coalesce(upper(block_range), 2147483647) int4_minmax_ops, vid int8_minmax_ops);
create index habitat_block_range_closed
    on "sgd0815"."habitat"(coalesce(upper(block_range), 2147483647))
 where coalesce(upper(block_range), 2147483647) < 2147483647;
create index attr_2_0_habitat_id
    on "sgd0815"."habitat" using btree("id");
create index attr_2_1_habitat_most_common
    on "sgd0815"."habitat" using gist("most_common", block_range);

"#;

const FORWARD_ENUM_GQL: &str = r#"
type Thing @entity  {
    id: ID!,
    orientation: Orientation!
}

enum Orientation {
    UP, DOWN
}
"#;

const FORWARD_ENUM_SQL: &str = r#"create type sgd0815."orientation"
    as enum ('DOWN', 'UP');
create table "sgd0815"."thing" (
        vid                  bigserial primary key,
        block_range          int4range not null,
        "id"                 text not null,
        "orientation"        "sgd0815"."orientation" not null
);
alter table "sgd0815"."thing"
  add constraint thing_id_block_range_excl exclude using gist (id with =, block_range with &&);
create index brin_thing
    on "sgd0815"."thing"
 using brin(lower(block_range) int4_minmax_ops, coalesce(upper(block_range), 2147483647) int4_minmax_ops, vid int8_minmax_ops);
create index thing_block_range_closed
    on "sgd0815"."thing"(coalesce(upper(block_range), 2147483647))
 where coalesce(upper(block_range), 2147483647) < 2147483647;
create index attr_0_0_thing_id
    on "sgd0815"."thing" using btree("id");
create index attr_0_1_thing_orientation
    on "sgd0815"."thing" using btree("orientation");

"#;
