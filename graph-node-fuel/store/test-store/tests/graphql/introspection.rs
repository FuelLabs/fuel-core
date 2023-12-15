use std::sync::Arc;
use std::time::Duration;

use graph::components::store::QueryPermit;
use graph::data::graphql::{object_value, ObjectOrInterface};
use graph::data::query::Trace;
use graph::prelude::{
    async_trait, o, r, s, serde_json, slog, tokio, DeploymentHash, Logger, Query,
    QueryExecutionError, QueryResult,
};
use graph::schema::{ApiSchema, InputSchema};

use graph_graphql::prelude::{
    a, execute_query, ExecutionContext, Query as PreparedQuery, QueryExecutionOptions, Resolver,
};
use test_store::graphql_metrics;

/// Mock resolver used in tests that don't need a resolver.
#[derive(Clone)]
pub struct MockResolver;

#[async_trait]
impl Resolver for MockResolver {
    const CACHEABLE: bool = false;

    fn prefetch(
        &self,
        _: &ExecutionContext<Self>,
        _: &a::SelectionSet,
    ) -> Result<(Option<r::Value>, Trace), Vec<QueryExecutionError>> {
        Ok((None, Trace::None))
    }

    async fn resolve_objects(
        &self,
        _: Option<r::Value>,
        _field: &a::Field,
        _field_definition: &s::Field,
        _object_type: ObjectOrInterface<'_>,
    ) -> Result<r::Value, QueryExecutionError> {
        Ok(r::Value::Null)
    }

    async fn resolve_object(
        &self,
        __: Option<r::Value>,
        _field: &a::Field,
        _field_definition: &s::Field,
        _object_type: ObjectOrInterface<'_>,
    ) -> Result<r::Value, QueryExecutionError> {
        Ok(r::Value::Null)
    }

    async fn query_permit(&self) -> Result<QueryPermit, QueryExecutionError> {
        let permit = Arc::new(tokio::sync::Semaphore::new(1))
            .acquire_owned()
            .await
            .unwrap();
        Ok(QueryPermit {
            permit,
            wait: Duration::from_secs(0),
        })
    }
}

fn api_schema(raw: &str, id: &str) -> Arc<ApiSchema> {
    let id = DeploymentHash::new(id).unwrap();
    let schema = InputSchema::parse(raw, id).unwrap().api_schema().unwrap();
    Arc::new(schema)
}

/// Creates a basic GraphQL schema that exercies scalars, directives,
/// enums, interfaces, input objects, object types and field arguments.
fn mock_schema() -> Arc<ApiSchema> {
    api_schema(
        "
             directive @language(
               language: String = \"English\"
             ) on FIELD_DEFINITION

             enum Role {
               USER
               ADMIN
             }

             interface Node {
               id: ID!
             }

             type User implements Node @entity {
               id: ID!
               name: String! @language(language: \"English\")
               role: Role!
             }
             ",
        "mockschema",
    )
}

/// Builds the expected result for GraphiQL's introspection query that we are
/// using for testing.
fn expected_mock_schema_introspection() -> r::Value {
    const JSON: &str = include_str!("mock_introspection.json");
    serde_json::from_str::<serde_json::Value>(JSON)
        .unwrap()
        .into()
}

/// Execute an introspection query.
async fn introspection_query(schema: Arc<ApiSchema>, query: &str) -> QueryResult {
    // Create the query
    let query = Query::new(
        graphql_parser::parse_query(query).unwrap().into_static(),
        None,
        false,
    );

    // Execute it
    let logger = Logger::root(slog::Discard, o!());
    let options = QueryExecutionOptions {
        resolver: MockResolver,
        deadline: None,
        max_first: std::u32::MAX,
        max_skip: std::u32::MAX,
        trace: false,
    };

    let result =
        match PreparedQuery::new(&logger, schema, None, query, None, 100, graphql_metrics()) {
            Ok(query) => {
                Ok(Arc::try_unwrap(execute_query(query, None, None, options).await).unwrap())
            }
            Err(e) => Err(e),
        };
    QueryResult::from(result)
}

/// Compare two values and consider them the same if they are identical with
/// some special treatment meant to mimic what GraphQL users care about in
/// the resulting JSON value
///
/// (1) Objects are the same if they have the same entries, regardless of
/// order (the PartialEq implementation for Object also requires entries to
/// be in the same order which should probably be fixed)
///
/// (2) Enums and Strings are the same if they have the same string value
#[track_caller]
fn same_value(a: &r::Value, b: &r::Value) -> bool {
    match a {
        r::Value::Int(_) | r::Value::Float(_) | r::Value::Boolean(_) | r::Value::Null => a == b,
        r::Value::List(la) => match b {
            r::Value::List(lb) => {
                if la.len() != lb.len() {
                    return false;
                }
                for i in 0..la.len() {
                    if !same_value(&la[i], &lb[i]) {
                        return false;
                    }
                }
                return true;
            }
            _ => false,
        },
        r::Value::String(sa) | r::Value::Enum(sa) => match b {
            r::Value::String(sb) | r::Value::Enum(sb) => sa == sb,
            _ => {
                println!("STRING/ENUM: {} != {}", sa, b);
                false
            }
        },
        r::Value::Object(oa) => match b {
            r::Value::Object(ob) => {
                if oa.len() != ob.len() {
                    return false;
                }
                for (ka, va) in oa.iter() {
                    match ob.get(ka) {
                        Some(vb) => {
                            if !same_value(va, vb) {
                                return false;
                            }
                        }
                        None => {
                            return false;
                        }
                    }
                }
                return true;
            }
            _ => false,
        },
    }
}

#[tokio::test]
async fn satisfies_graphiql_introspection_query_without_fragments() {
    let result = introspection_query(
        mock_schema(),
        "
      query IntrospectionQuery {
        __schema {
          queryType { name }
          mutationType { name }
          subscriptionType { name}
          types {
            kind
            name
            description
            fields(includeDeprecated: true) {
              name
              description
              args {
                name
                description
                type {
                  kind
                  name
                  ofType {
                    kind
                    name
                    ofType {
                      kind
                      name
                      ofType {
                        kind
                        name
                        ofType {
                          kind
                          name
                          ofType {
                            kind
                            name
                            ofType {
                              kind
                              name
                              ofType {
                                kind
                                name
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
                defaultValue
              }
              type {
                kind
                name
                ofType {
                  kind
                  name
                  ofType {
                    kind
                    name
                    ofType {
                      kind
                      name
                      ofType {
                        kind
                        name
                        ofType {
                          kind
                          name
                          ofType {
                            kind
                            name
                            ofType {
                              kind
                              name
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
              isDeprecated
              deprecationReason
            }
            inputFields {
              name
              description
              type {
                kind
                name
                ofType {
                  kind
                  name
                  ofType {
                    kind
                    name
                    ofType {
                      kind
                      name
                      ofType {
                        kind
                        name
                        ofType {
                          kind
                          name
                          ofType {
                            kind
                            name
                            ofType {
                              kind
                              name
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
              defaultValue
            }
            interfaces {
              kind
              name
              ofType {
                kind
                name
                ofType {
                  kind
                  name
                  ofType {
                    kind
                    name
                    ofType {
                      kind
                      name
                      ofType {
                        kind
                        name
                        ofType {
                          kind
                          name
                          ofType {
                            kind
                            name
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
            enumValues(includeDeprecated: true) {
              name
              description
              isDeprecated
              deprecationReason
            }
            possibleTypes {
              kind
              name
              ofType {
                kind
                name
                ofType {
                  kind
                  name
                  ofType {
                    kind
                    name
                    ofType {
                      kind
                      name
                      ofType {
                        kind
                        name
                        ofType {
                          kind
                          name
                          ofType {
                            kind
                            name
                          }
                        }
                      }
                    }
                  }
                }
              }
           }
          }
          directives {
            name
            description
            locations
            args {
              name
              description
              type {
                kind
                name
                ofType {
                  kind
                  name
                  ofType {
                    kind
                    name
                    ofType {
                      kind
                      name
                      ofType {
                        kind
                        name
                        ofType {
                          kind
                          name
                          ofType {
                            kind
                            name
                            ofType {
                              kind
                              name
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
              defaultValue
            }
          }
        }
      }
    ",
    )
    .await;

    let data = result
        .to_result()
        .expect("Introspection query returned no result")
        .unwrap();
    assert!(same_value(&data, &expected_mock_schema_introspection()));
}

#[tokio::test]
async fn satisfies_graphiql_introspection_query_with_fragments() {
    let result = introspection_query(
        mock_schema(),
        "
      query IntrospectionQuery {
        __schema {
          queryType { name }
          mutationType { name }
          subscriptionType { name }
          types {
            ...FullType
          }
          directives {
            name
            description
            locations
            args {
              ...InputValue
            }
          }
        }
      }

      fragment FullType on __Type {
        kind
        name
        description
        fields(includeDeprecated: true) {
          name
          description
          args {
            ...InputValue
          }
          type {
            ...TypeRef
          }
          isDeprecated
          deprecationReason
        }
        inputFields {
          ...InputValue
        }
        interfaces {
          ...TypeRef
        }
        enumValues(includeDeprecated: true) {
          name
          description
          isDeprecated
          deprecationReason
        }
        possibleTypes {
          ...TypeRef
        }
      }

      fragment InputValue on __InputValue {
        name
        description
        type { ...TypeRef }
        defaultValue
      }

      fragment TypeRef on __Type {
        kind
        name
        ofType {
          kind
          name
          ofType {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
                ofType {
                  kind
                  name
                  ofType {
                    kind
                    name
                    ofType {
                      kind
                      name
                    }
                  }
                }
              }
            }
          }
        }
      }
    ",
    )
    .await;

    let data = result
        .to_result()
        .expect("Introspection query returned no result")
        .unwrap();

    // If the data needed for expected_mock_schema_introspection() ever
    // needs to be regenerated, uncomment this line, and save the output in
    // mock_introspection.json
    //
    // println!("{}", graph::prelude::serde_json::to_string(&data).unwrap());
    assert!(same_value(&data, &expected_mock_schema_introspection()));
}

const COMPLEX_SCHEMA: &str = "
enum RegEntryStatus {
  regEntry_status_challengePeriod
  regEntry_status_commitPeriod
  regEntry_status_revealPeriod
  regEntry_status_blacklisted
  regEntry_status_whitelisted
}

interface RegEntry {
  regEntry_address: ID
  regEntry_version: Int
  regEntry_status: RegEntryStatus
  regEntry_creator: User
  regEntry_deposit: Int
  regEntry_createdOn: String
  regEntry_challengePeriodEnd: String
  challenge_challenger: User
  challenge_createdOn: String
  challenge_comment: String
  challenge_votingToken: String
  challenge_rewardPool: Int
  challenge_commitPeriodEnd: String
  challenge_revealPeriodEnd: String
  challenge_votesFor: Int
  challenge_votesAgainst: Int
  challenge_votesTotal: Int
  challenge_claimedRewardOn: String
  challenge_vote(vote_voter: ID!): Vote
}

enum VoteOption {
  voteOption_noVote
  voteOption_voteFor
  voteOption_voteAgainst
}

type Vote @entity {
  id: ID!
  vote_secretHash: String
  vote_option: VoteOption
  vote_amount: Int
  vote_revealedOn: String
  vote_claimedRewardOn: String
  vote_reward: Int
}

type Meme implements RegEntry @entity {
  id: ID!
  regEntry_address: ID
  regEntry_version: Int
  regEntry_status: RegEntryStatus
  regEntry_creator: User
  regEntry_deposit: Int
  regEntry_createdOn: String
  regEntry_challengePeriodEnd: String
  challenge_challenger: User
  challenge_createdOn: String
  challenge_comment: String
  challenge_votingToken: String
  challenge_rewardPool: Int
  challenge_commitPeriodEnd: String
  challenge_revealPeriodEnd: String
  challenge_votesFor: Int
  challenge_votesAgainst: Int
  challenge_votesTotal: Int
  challenge_claimedRewardOn: String
  challenge_vote(vote_voter: ID!): Vote
  # Balance of voting token of a voter. This is client-side only, server doesn't return this
  challenge_availableVoteAmount(voter: ID!): Int
  meme_title: String
  meme_number: Int
  meme_metaHash: String
  meme_imageHash: String
  meme_totalSupply: Int
  meme_totalMinted: Int
  meme_tokenIdStart: Int
  meme_totalTradeVolume: Int
  meme_totalTradeVolumeRank: Int
  meme_ownedMemeTokens(owner: String): [MemeToken]
  meme_tags: [Tag]
}

type Tag  @entity {
  id: ID!
  tag_id: ID
  tag_name: String
}

type MemeToken @entity {
  id: ID!
  memeToken_tokenId: ID
  memeToken_number: Int
  memeToken_owner: User
  memeToken_meme: Meme
}

enum MemeAuctionStatus {
  memeAuction_status_active
  memeAuction_status_canceled
  memeAuction_status_done
}

type MemeAuction @entity {
  id: ID!
  memeAuction_address: ID
  memeAuction_seller: User
  memeAuction_buyer: User
  memeAuction_startPrice: Int
  memeAuction_endPrice: Int
  memeAuction_duration: Int
  memeAuction_startedOn: String
  memeAuction_boughtOn: String
  memeAuction_status: MemeAuctionStatus
  memeAuction_memeToken: MemeToken
}

type ParamChange implements RegEntry @entity {
  id: ID!
  regEntry_address: ID
  regEntry_version: Int
  regEntry_status: RegEntryStatus
  regEntry_creator: User
  regEntry_deposit: Int
  regEntry_createdOn: String
  regEntry_challengePeriodEnd: String
  challenge_challenger: User
  challenge_createdOn: String
  challenge_comment: String
  challenge_votingToken: String
  challenge_rewardPool: Int
  challenge_commitPeriodEnd: String
  challenge_revealPeriodEnd: String
  challenge_votesFor: Int
  challenge_votesAgainst: Int
  challenge_votesTotal: Int
  challenge_claimedRewardOn: String
  challenge_vote(vote_voter: ID!): Vote
  # Balance of voting token of a voter. This is client-side only, server doesn't return this
  challenge_availableVoteAmount(voter: ID!): Int
  paramChange_db: String
  paramChange_key: String
  paramChange_value: Int
  paramChange_originalValue: Int
  paramChange_appliedOn: String
}

type User @entity {
  id: ID!
  # Ethereum address of an user
  user_address: ID
  # Total number of memes submitted by user
  user_totalCreatedMemes: Int
  # Total number of memes submitted by user, which successfully got into TCR
  user_totalCreatedMemesWhitelisted: Int
  # Largest sale creator has done with his newly minted meme
  user_creatorLargestSale: MemeAuction
  # Position of a creator in leaderboard according to user_totalCreatedMemesWhitelisted
  user_creatorRank: Int
  # Amount of meme tokenIds owned by user
  user_totalCollectedTokenIds: Int
  # Amount of unique memes owned by user
  user_totalCollectedMemes: Int
  # Largest auction user sold, in terms of price
  user_largestSale: MemeAuction
  # Largest auction user bought into, in terms of price
  user_largestBuy: MemeAuction
  # Amount of challenges user created
  user_totalCreatedChallenges: Int
  # Amount of challenges user created and ended up in his favor
  user_totalCreatedChallengesSuccess: Int
  # Total amount of DANK token user received from challenger rewards
  user_challengerTotalEarned: Int
  # Total amount of DANK token user received from challenger rewards
  user_challengerRank: Int
  # Amount of different votes user participated in
  user_totalParticipatedVotes: Int
  # Amount of different votes user voted for winning option
  user_totalParticipatedVotesSuccess: Int
  # Amount of DANK token user received for voting for winning option
  user_voterTotalEarned: Int
  # Position of voter in leaderboard according to user_voterTotalEarned
  user_voterRank: Int
  # Sum of user_challengerTotalEarned and user_voterTotalEarned
  user_curatorTotalEarned: Int
  # Position of curator in leaderboard according to user_curatorTotalEarned
  user_curatorRank: Int
}

type Parameter @entity {
  id: ID!
  param_db: ID
  param_key: ID
  param_value: Int
}
";

#[tokio::test]
async fn successfully_runs_introspection_query_against_complex_schema() {
    let schema = api_schema(COMPLEX_SCHEMA, "complexschema");

    let result = introspection_query(
        schema,
        "
        query IntrospectionQuery {
          __schema {
            queryType { name }
            mutationType { name }
            subscriptionType { name }
            types {
              ...FullType
            }
            directives {
              name
              description
              locations
              args {
                ...InputValue
              }
            }
          }
        }

        fragment FullType on __Type {
          kind
          name
          description
          fields(includeDeprecated: true) {
            name
            description
            args {
              ...InputValue
            }
            type {
              ...TypeRef
            }
            isDeprecated
            deprecationReason
          }
          inputFields {
            ...InputValue
          }
          interfaces {
            ...TypeRef
          }
          enumValues(includeDeprecated: true) {
            name
            description
            isDeprecated
            deprecationReason
          }
          possibleTypes {
            ...TypeRef
          }
        }

        fragment InputValue on __InputValue {
          name
          description
          type { ...TypeRef }
          defaultValue
        }

        fragment TypeRef on __Type {
          kind
          name
          ofType {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
                ofType {
                  kind
                  name
                  ofType {
                    kind
                    name
                    ofType {
                      kind
                      name
                      ofType {
                        kind
                        name
                      }
                    }
                  }
                }
              }
            }
          }
        }
    ",
    )
    .await;

    assert!(!result.has_errors(), "{:#?}", result);
}

#[tokio::test]
async fn introspection_possible_types() {
    let schema = api_schema(COMPLEX_SCHEMA, "complexschema");

    // Test "possibleTypes" introspection in interfaces
    let response = introspection_query(
        schema,
        "query {
          __type(name: \"RegEntry\") {
              name
              possibleTypes {
                name
              }
          }
        }",
    )
    .await
    .to_result()
    .unwrap()
    .unwrap();

    assert_eq!(
        response,
        object_value(vec![(
            "__type",
            object_value(vec![
                ("name", r::Value::String("RegEntry".to_string())),
                (
                    "possibleTypes",
                    r::Value::List(vec![
                        object_value(vec![("name", r::Value::String("Meme".to_owned()))]),
                        object_value(vec![("name", r::Value::String("ParamChange".to_owned()))])
                    ])
                )
            ])
        )])
    )
}
