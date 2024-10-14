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

static HELP_STRING: Lazy<String> = Lazy::new(|| {
    let all_modules: Vec<_> = Module::iter().map(|module| module.to_string()).collect();
    format!(
        "Comma-separated list of modules or 'all' to disable all metrics. Available options: {}, all",
        all_modules.join(", ")
    )
});

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

    fn assert_config(config: &Config, expected_disabled: Vec<Module>) {
        expected_disabled
            .iter()
            .for_each(|module| assert!(!config.is_enabled(*module)));

        let expected_enabled: Vec<_> = Module::iter()
            .filter(|module| !expected_disabled.contains(module))
            .collect();
        expected_enabled
            .iter()
            .for_each(|module| assert!(config.is_enabled(*module)));
    }

    #[test]
    fn metrics_config() {
        const EXCLUDED_METRICS: &str = "importer,txpool";

        let expected_disabled = [Module::Importer, Module::TxPool].to_vec();

        let config: Config = EXCLUDED_METRICS.into();
        assert_config(&config, expected_disabled);
    }

    #[test]
    fn metrics_config_with_incorrect_values() {
        const EXCLUDED_METRICS: &str = "txpool,alpha,bravo";

        let expected_disabled = [Module::TxPool].to_vec();

        let config: Config = EXCLUDED_METRICS.into();
        assert_config(&config, expected_disabled);
    }

    #[test]
    fn metrics_config_with_empty_value() {
        const EXCLUDED_METRICS: &str = "";

        let expected_disabled = vec![];

        let config: Config = EXCLUDED_METRICS.into();
        assert_config(&config, expected_disabled);
    }

    #[test]
    fn metrics_config_supports_all() {
        const EXCLUDED_METRICS: &str = "all";

        let expected_disabled = Module::iter().collect::<Vec<_>>();

        let config: Config = EXCLUDED_METRICS.into();
        assert_config(&config, expected_disabled);
    }
}
