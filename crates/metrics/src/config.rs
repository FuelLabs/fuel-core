#[cfg(test)]
use itertools as _;
#[cfg(not(test))]
use itertools::Itertools;
#[cfg(test)]
use once_cell as _;
#[cfg(not(test))]
use once_cell::sync::Lazy;
use strum::IntoEnumIterator;
use strum_macros::{
    Display,
    EnumIter,
    EnumString,
};

#[derive(Debug, Display, Clone, Copy, PartialEq, EnumString, EnumIter)]
#[strum(serialize_all = "lowercase")]
pub enum Module {
    Importer,
    P2P,
    Producer,
    TxPool, /* TODO[RC]: Not used. Add support in https://github.com/FuelLabs/fuel-core/pull/2321 */
    GraphQL, // TODO[RC]: Not used... yet.
}

#[derive(Debug, Clone, Default)]
pub struct Config(Vec<Module>);
impl Config {
    pub fn is_enabled(&self, module: Module) -> bool {
        !self.0.contains(&module)
    }
}

impl std::convert::From<&str> for Config {
    fn from(s: &str) -> Self {
        if s == "all" {
            return Self(Module::iter().collect())
        }
        Self(
            s.split(',')
                .filter_map(|s| s.parse::<Module>().ok())
                .collect(),
        )
    }
}

#[cfg(not(test))]
static HELP_STRING: Lazy<String> = Lazy::new(|| {
    format!(
        "Comma-separated list of modules or 'all' to disable all metrics. Available options: {}, all",
        Module::iter().join(", ")
    )
});

#[cfg(not(test))]
pub fn help_string() -> &'static str {
    &HELP_STRING
}

#[cfg(test)]
mod tests {
    use strum::IntoEnumIterator;

    use crate::config::{
        Config,
        Module,
    };

    #[test]
    fn metrics_config() {
        const EXCLUDED_METRICS: &str = "importer,txpool";

        let config: Config = EXCLUDED_METRICS.into();
        assert!(!config.is_enabled(Module::Importer));
        assert!(!config.is_enabled(Module::TxPool));
        assert!(config.is_enabled(Module::P2P));
        assert!(config.is_enabled(Module::Producer));
    }

    #[test]
    fn metrics_config_with_incorrect_values() {
        const EXCLUDED_METRICS: &str = "txpool,alpha,bravo";

        let config: Config = EXCLUDED_METRICS.into();
        assert!(!config.is_enabled(Module::TxPool));
        assert!(config.is_enabled(Module::P2P));
    }

    #[test]
    fn metrics_config_with_empty_value() {
        const EXCLUDED_METRICS: &str = "";

        let config: Config = EXCLUDED_METRICS.into();
        assert!(config.is_enabled(Module::TxPool));
        assert!(config.is_enabled(Module::P2P));
    }

    #[test]
    fn metrics_config_supports_all() {
        const EXCLUDED_METRICS: &str = "all";

        let config: Config = EXCLUDED_METRICS.into();
        for module in Module::iter() {
            assert!(!config.is_enabled(module));
        }
    }
}
