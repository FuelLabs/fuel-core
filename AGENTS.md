# AGENTS.md

Guidance for AI coding agents working in this repository (`FuelLabs/fuel-core`).

## Required quality gate (do not skip)

Before claiming done, committing, or pushing a PR that touches Rust sources, agents **must**:

1. Format with the **same nightly rustfmt CI uses** (not the pin in `rust-toolchain.toml`).
2. Confirm the check is clean.

```bash
# Install once if missing
rustup toolchain install nightly-2025-09-28 --component rustfmt

# Apply + verify (matches `.github/workflows/ci.yml` `rustfmt` job)
cargo +nightly-2025-09-28 fmt --all
cargo +nightly-2025-09-28 fmt --all -- --check
```

Keep `RUST_VERSION_FMT` in sync with `.github/workflows/ci.yml` (`env.RUST_VERSION_FMT`, currently `nightly-2025-09-28`).

### Hard rules

- **Never** run `cargo fmt` / `cargo fmt --all` on the **stable / 1.94.1** toolchain from `rust-toolchain.toml`. `.rustfmt.toml` uses nightly-only options (`imports_layout`, `imports_granularity`, `normalize_comments`, `trailing_semicolon`). Stable ignores them and can rewrite hundreds of files with compact import layout.
- Prefer formatting the whole workspace with the CI nightly (`cargo +nightly-2025-09-28 fmt --all`) so the PR matches CI. If you must format a subset, still use that nightly + this repo's `.rustfmt.toml`.
- Also run `cargo clippy` (stable pin) with `-D warnings` on touched crates when practical; CI enforces warnings as errors.

## Commands (matrix)

| Step | Command |
| :--- | :--- |
| Setup | `cargo fetch` |
| Build | `cargo build` |
| Test | `cargo test` |
| **Format (required)** | `cargo +nightly-2025-09-28 fmt --all` then `-- --check` |
| Lint | `cargo clippy -- -D warnings` (scoped to changed crates is OK) |
| Local CI-ish | `source ci_checks.sh` |

## Domain / readiness language

See `CONTEXT.md` for Ready vs Health, Height Gap, and Max Sync Height Diff.
