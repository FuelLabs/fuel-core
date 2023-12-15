use std::collections::HashSet;
use std::convert::TryFrom;

use crate::data::graphql::{DirectiveExt, ValueExt};
use crate::prelude::s;

#[derive(Clone, Debug, PartialEq)]
pub enum FulltextLanguage {
    Simple,
    Danish,
    Dutch,
    English,
    Finnish,
    French,
    German,
    Hungarian,
    Italian,
    Norwegian,
    Portugese,
    Romanian,
    Russian,
    Spanish,
    Swedish,
    Turkish,
}

impl TryFrom<&str> for FulltextLanguage {
    type Error = String;
    fn try_from(language: &str) -> Result<Self, Self::Error> {
        match language {
            "simple" => Ok(FulltextLanguage::Simple),
            "da" => Ok(FulltextLanguage::Danish),
            "nl" => Ok(FulltextLanguage::Dutch),
            "en" => Ok(FulltextLanguage::English),
            "fi" => Ok(FulltextLanguage::Finnish),
            "fr" => Ok(FulltextLanguage::French),
            "de" => Ok(FulltextLanguage::German),
            "hu" => Ok(FulltextLanguage::Hungarian),
            "it" => Ok(FulltextLanguage::Italian),
            "no" => Ok(FulltextLanguage::Norwegian),
            "pt" => Ok(FulltextLanguage::Portugese),
            "ro" => Ok(FulltextLanguage::Romanian),
            "ru" => Ok(FulltextLanguage::Russian),
            "es" => Ok(FulltextLanguage::Spanish),
            "sv" => Ok(FulltextLanguage::Swedish),
            "tr" => Ok(FulltextLanguage::Turkish),
            invalid => Err(format!(
                "Provided language for fulltext search is invalid: {}",
                invalid
            )),
        }
    }
}

impl FulltextLanguage {
    /// Return the language as a valid SQL string. The string is safe to
    /// directly use verbatim in a query, i.e., doesn't require being passed
    /// through a bind variable
    pub fn as_sql(&self) -> &'static str {
        match self {
            Self::Simple => "'simple'",
            Self::Danish => "'danish'",
            Self::Dutch => "'dutch'",
            Self::English => "'english'",
            Self::Finnish => "'finnish'",
            Self::French => "'french'",
            Self::German => "'german'",
            Self::Hungarian => "'hungarian'",
            Self::Italian => "'italian'",
            Self::Norwegian => "'norwegian'",
            Self::Portugese => "'portugese'",
            Self::Romanian => "'romanian'",
            Self::Russian => "'russian'",
            Self::Spanish => "'spanish'",
            Self::Swedish => "'swedish'",
            Self::Turkish => "'turkish'",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum FulltextAlgorithm {
    Rank,
    ProximityRank,
}

impl TryFrom<&str> for FulltextAlgorithm {
    type Error = String;
    fn try_from(algorithm: &str) -> Result<Self, Self::Error> {
        match algorithm {
            "rank" => Ok(FulltextAlgorithm::Rank),
            "proximityRank" => Ok(FulltextAlgorithm::ProximityRank),
            invalid => Err(format!(
                "The provided fulltext search algorithm {} is invalid. It must be one of: rank, proximityRank",
                invalid,
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FulltextConfig {
    pub language: FulltextLanguage,
    pub algorithm: FulltextAlgorithm,
}

pub struct FulltextDefinition {
    pub config: FulltextConfig,
    pub included_fields: HashSet<String>,
    pub name: String,
}

impl From<&s::Directive> for FulltextDefinition {
    // Assumes the input is a Fulltext Directive that has already been validated because it makes
    // liberal use of unwrap() where specific types are expected
    fn from(directive: &s::Directive) -> Self {
        let name = directive.argument("name").unwrap().as_str().unwrap();

        let algorithm = FulltextAlgorithm::try_from(
            directive.argument("algorithm").unwrap().as_enum().unwrap(),
        )
        .unwrap();

        let language =
            FulltextLanguage::try_from(directive.argument("language").unwrap().as_enum().unwrap())
                .unwrap();

        let included_entity_list = directive.argument("include").unwrap().as_list().unwrap();
        // Currently fulltext query fields are limited to 1 entity, so we just take the first (and only) included Entity
        let included_entity = included_entity_list.first().unwrap().as_object().unwrap();
        let included_field_values = included_entity.get("fields").unwrap().as_list().unwrap();
        let included_fields: HashSet<String> = included_field_values
            .iter()
            .map(|field| {
                field
                    .as_object()
                    .unwrap()
                    .get("name")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .into()
            })
            .collect();

        FulltextDefinition {
            config: FulltextConfig {
                language,
                algorithm,
            },
            included_fields,
            name: name.into(),
        }
    }
}
