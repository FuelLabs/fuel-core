use clap::Args;
#[cfg(feature = "production")]
use fuel_core::service::sub_services::DEFAULT_GAS_PRICE_CHANGE_PERCENT;
use fuel_core_types::clamped_percentage::ClampedPercentage;
use url::Url;

#[derive(Debug, Clone, Args)]
pub struct GasPriceArgs {
    /// The starting execution gas price for the network
    #[cfg_attr(
        feature = "production",
        arg(long = "starting-gas-price", default_value = "1000", env)
    )]
    #[cfg_attr(
        not(feature = "production"),
        arg(long = "starting-gas-price", default_value = "0", env)
    )]
    pub starting_gas_price: u64,

    /// The percentage change in gas price per block
    #[cfg_attr(
        feature = "production",
        arg(long = "gas-price-change-percent", default_value_t = DEFAULT_GAS_PRICE_CHANGE_PERCENT, value_parser = parse_clamped_percentage, env)
    )]
    #[cfg_attr(
        not(feature = "production"),
        arg(long = "gas-price-change-percent", default_value = "0", value_parser = parse_clamped_percentage, env)
    )]
    pub gas_price_change_percent: ClampedPercentage,

    /// The minimum allowed gas price
    #[arg(long = "min-gas-price", default_value = "0", env)]
    pub min_gas_price: u64,

    /// The percentage threshold for gas price increase
    #[arg(long = "gas-price-threshold-percent", default_value = "50", value_parser = parse_clamped_percentage, env)]
    pub gas_price_threshold_percent: ClampedPercentage,

    /// Minimum DA gas price
    #[cfg_attr(
        feature = "production",
        arg(long = "min-da-gas-price", default_value = "1000", env)
    )]
    #[cfg_attr(
        not(feature = "production"),
        arg(long = "min-da-gas-price", default_value = "0", env)
    )]
    pub min_da_gas_price: u64,

    /// Maximum allowed gas price for DA.
    #[arg(long = "max-da-gas-price", default_value = "100000", env)]
    pub max_da_gas_price: u64,

    /// P component of DA gas price calculation
    /// **NOTE**: This is the **inverse** gain of a typical P controller.
    /// Increasing this value will reduce gas price fluctuations.
    #[arg(
        long = "da-gas-price-p-component",
        default_value = "799999999999993",
        env
    )]
    pub da_gas_price_p_component: i64,

    /// D component of DA gas price calculation
    /// **NOTE**: This is the **inverse** anticipatory control factor of a typical PD controller.
    /// Increasing this value will reduce the dampening effect of quick algorithm changes.
    #[arg(
        long = "da-gas-price-d-component",
        default_value = "10000000000000000",
        env
    )]
    pub da_gas_price_d_component: i64,

    /// The URL for the DA Block Committer info
    #[arg(long = "da-committer-url", env)]
    pub da_committer_url: Option<Url>,

    /// The interval at which the `DaSourceService` polls for new data
    #[arg(long = "da-poll-interval", env)]
    pub da_poll_interval: Option<humantime::Duration>,

    /// The L2 height the Gas Price Service will assume is already recorded on DA
    /// i.e. If you want the Gas Price Service to look for the costs of block 1000, set to 999
    #[arg(long = "da-starting-recorded-height", env)]
    pub da_starting_recorded_height: Option<u32>,
}

fn parse_clamped_percentage(
    s: &str,
) -> Result<ClampedPercentage, std::num::ParseIntError> {
    let value = s.parse::<u8>()?;
    Ok(ClampedPercentage::new(value))
}
