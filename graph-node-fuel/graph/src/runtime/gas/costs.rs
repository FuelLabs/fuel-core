//! Stores all the gas costs is one place so they can be compared easily.
//! Determinism: Once deployed, none of these values can be changed without a version upgrade.

use super::*;

/// Using 10 gas = ~1ns for WASM instructions.
const GAS_PER_SECOND: u64 = 10_000_000_000;

/// Set max gas to 1000 seconds worth of gas per handler. The intent here is to have the determinism
/// cutoff be very high, while still allowing more reasonable timer based cutoffs. Having a unit
/// like 10 gas for ~1ns allows us to be granular in instructions which are aggregated into metered
/// blocks via https://docs.rs/pwasm-utils/0.16.0/pwasm_utils/fn.inject_gas_counter.html But we can
/// still charge very high numbers for other things.
pub const CONST_MAX_GAS_PER_HANDLER: u64 = 1000 * GAS_PER_SECOND;

/// Gas for instructions are aggregated into blocks, so hopefully gas calls each have relatively
/// large gas. But in the case they don't, we don't want the overhead of calling out into a host
/// export to be the dominant cost that causes unexpectedly high execution times.
///
/// This value is based on the benchmark of an empty infinite loop, which does basically nothing
/// other than call the gas function. The benchmark result was closer to 5000 gas but use 10_000 to
/// be conservative.
pub const HOST_EXPORT_GAS: Gas = Gas(10_000);

/// As a heuristic for the cost of host fns it makes sense to reason in terms of bandwidth and
/// calculate the cost from there. Because we don't have benchmarks for each host fn, we go with
/// pessimistic assumption of performance of 10 MB/s, which nonetheless allows for 10 GB to be
/// processed through host exports by a single handler at a 1000 seconds budget.
const DEFAULT_BYTE_PER_SECOND: u64 = 10_000_000;

/// With the current parameters DEFAULT_GAS_PER_BYTE = 1_000.
const DEFAULT_GAS_PER_BYTE: u64 = GAS_PER_SECOND / DEFAULT_BYTE_PER_SECOND;

/// Base gas cost for calling any host export.
/// Security: This must be non-zero.
pub const DEFAULT_BASE_COST: u64 = 100_000;

pub const DEFAULT_GAS_OP: GasOp = GasOp {
    base_cost: DEFAULT_BASE_COST,
    size_mult: DEFAULT_GAS_PER_BYTE,
};

/// Because big math has a multiplicative complexity, that can result in high sizes, so assume a
/// bandwidth of 100 MB/s, faster than the default.
const BIG_MATH_BYTE_PER_SECOND: u64 = 100_000_000;
const BIG_MATH_GAS_PER_BYTE: u64 = GAS_PER_SECOND / BIG_MATH_BYTE_PER_SECOND;

pub const BIG_MATH_GAS_OP: GasOp = GasOp {
    base_cost: DEFAULT_BASE_COST,
    size_mult: BIG_MATH_GAS_PER_BYTE,
};

// Allow up to 100,000 data sources to be created
pub const CREATE_DATA_SOURCE: Gas = Gas(CONST_MAX_GAS_PER_HANDLER / 100_000);

pub const ENS_NAME_BY_HASH: Gas = Gas(DEFAULT_BASE_COST);

pub const LOG_OP: GasOp = GasOp {
    // Allow up to 100,000 logs
    base_cost: CONST_MAX_GAS_PER_HANDLER / 100_000,
    size_mult: DEFAULT_GAS_PER_BYTE,
};

// Saving to the store is one of the most expensive operations.
pub const STORE_SET: GasOp = GasOp {
    // Allow up to 250k entities saved.
    base_cost: CONST_MAX_GAS_PER_HANDLER / 250_000,
    // If the size roughly corresponds to bytes, allow 1GB to be saved.
    size_mult: CONST_MAX_GAS_PER_HANDLER / 1_000_000_000,
};

// Reading from the store is much cheaper than writing.
pub const STORE_GET: GasOp = GasOp {
    base_cost: CONST_MAX_GAS_PER_HANDLER / 10_000_000,
    size_mult: CONST_MAX_GAS_PER_HANDLER / 10_000_000_000,
};

pub const STORE_REMOVE: GasOp = STORE_SET;

// Deeply nested JSON can take over 100x the memory of the serialized format, so multiplying the
// size cost by 100 makes sense.
pub const JSON_FROM_BYTES: GasOp = GasOp {
    base_cost: DEFAULT_BASE_COST,
    size_mult: DEFAULT_GAS_PER_BYTE * 100,
};
