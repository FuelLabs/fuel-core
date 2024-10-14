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
    All,
    Importer,
    P2P,
    Producer,
    TxPool, /* TODO[RC]: Not used. Add support in https://github.com/FuelLabs/fuel-core/pull/2321 */
    GraphQL, // TODO[RC]: Not used... yet.
}

/// Configuration for disabling metrics.
pub trait DisableConfig {
    /// Returns `true` if the given module is enabled.
    fn is_enabled(&self, module: Module) -> bool;
}

impl DisableConfig for Vec<Module> {
    fn is_enabled(&self, module: Module) -> bool {
        !self.contains(&module) && !self.contains(&Module::All)
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
