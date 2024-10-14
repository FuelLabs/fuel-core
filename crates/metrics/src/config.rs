use once_cell::sync::Lazy;
use strum::IntoEnumIterator;
use strum_macros::{
    Display,
    EnumIter,
    EnumString,
};

#[derive(Debug, derive_more::Display)]
pub enum ConfigCreationError {
    #[display(fmt = "No such module: {}", _0)]
    UnknownModule(String),
}

#[derive(Debug, Display, Clone, Copy, PartialEq, EnumString, EnumIter)]
#[strum(serialize_all = "lowercase")]
pub enum Module {
    Importer,
    P2P,
    Producer,
    TxPool,
    GraphQL, // TODO[RC]: Not used... yet.
}

#[derive(Debug, Clone, Default)]
pub struct Config(Vec<Module>);
impl Config {
    pub fn is_enabled(&self, module: Module) -> bool {
        !self.0.contains(&module)
    }
}

impl std::fmt::Display for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let disabled_modules = self
            .0
            .iter()
            .map(|module| module.to_string())
            .collect::<Vec<_>>();
        write!(f, "Disable modules: {}", disabled_modules.join(", "))
    }
}

impl std::str::FromStr for Config {
    // TODO: Figure out how to make `clap` work directly with `ConfigCreationError`
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "all" || s.is_empty() {
            return Ok(Self(Module::iter().collect()))
        }
        let mut modules = vec![];

        for token in s.split(',') {
            let parsed_module = token.parse::<Module>().map_err(|_| {
                ConfigCreationError::UnknownModule(token.to_string()).to_string()
            })?;
            modules.push(parsed_module);
        }

        Ok(Self(modules))
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
    use std::str::FromStr;

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

        let config = Config::from_str(EXCLUDED_METRICS).expect("should create config");
        assert_config(&config, expected_disabled);
    }

    #[test]
    fn metrics_config_with_incorrect_values() {
        const EXCLUDED_METRICS: &str = "txpool,alpha,bravo";

        let config = Config::from_str(EXCLUDED_METRICS);
        assert!(matches!(config, Err(err) if err == "No such module: alpha"));
    }

    #[test]
    fn metrics_config_with_empty_value() {
        // This case is still possible if someone calls `--disable-metrics ""`
        const EXCLUDED_METRICS: &str = "";

        let expected_disabled = Module::iter().collect::<Vec<_>>();

        let config = Config::from_str(EXCLUDED_METRICS).expect("should create config");
        assert_config(&config, expected_disabled);
    }

    #[test]
    fn metrics_config_supports_all() {
        const EXCLUDED_METRICS: &str = "all";

        let expected_disabled = Module::iter().collect::<Vec<_>>();

        let config = Config::from_str(EXCLUDED_METRICS).expect("should create config");
        assert_config(&config, expected_disabled);
    }
}
