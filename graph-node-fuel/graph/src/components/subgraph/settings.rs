//! Facilities for dealing with subgraph-specific settings
use std::fs::read_to_string;

use crate::{
    anyhow,
    prelude::{regex::Regex, SubgraphName},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Predicate {
    #[serde(alias = "name", with = "serde_regex")]
    Name(Regex),
}

impl Predicate {
    fn matches(&self, name: &SubgraphName) -> bool {
        match self {
            Predicate::Name(rx) => rx.is_match(name.as_str()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Setting {
    #[serde(alias = "match")]
    pred: Predicate,
    pub history_blocks: i32,
}

impl Setting {
    fn matches(&self, name: &SubgraphName) -> bool {
        self.pred.matches(name)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Settings {
    #[serde(alias = "setting")]
    settings: Vec<Setting>,
}

impl Settings {
    pub fn from_file(path: &str) -> Result<Self, anyhow::Error> {
        Self::from_str(&read_to_string(path)?)
    }

    pub fn from_str(toml: &str) -> Result<Self, anyhow::Error> {
        toml::from_str::<Self>(toml).map_err(anyhow::Error::from)
    }

    pub fn for_name(&self, name: &SubgraphName) -> Option<&Setting> {
        self.settings.iter().find(|setting| setting.matches(name))
    }
}

#[cfg(test)]
mod test {
    use super::{Predicate, Settings};

    #[test]
    fn parses_correctly() {
        let content = r#"
        [[setting]]
        match = { name = ".*" }
        history_blocks = 10000

        [[setting]]
        match = { name = "xxxxx" }
        history_blocks = 10000

        [[setting]]
        match = { name = ".*!$" }
        history_blocks = 10000
        "#;

        let section = Settings::from_str(content).unwrap();
        assert_eq!(section.settings.len(), 3);

        let rule1 = match &section.settings[0].pred {
            Predicate::Name(name) => name,
        };
        assert_eq!(rule1.as_str(), ".*");

        let rule2 = match &section.settings[1].pred {
            Predicate::Name(name) => name,
        };
        assert_eq!(rule2.as_str(), "xxxxx");
        let rule1 = match &section.settings[2].pred {
            Predicate::Name(name) => name,
        };
        assert_eq!(rule1.as_str(), ".*!$");
    }
}
