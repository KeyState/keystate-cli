# GitHub Actions — Workflows

This directory contains the CI/CD pipelines for `keystate-cli`.
There are two workflows; both run on the same GNU/Linux runner
(`ubuntu-latest`) with the stable Rust toolchain, and both share the caching
from `Swatinem/rust-cache`. They mirror the `keystate-core` and
`keystate-adapter-keycloak` pipelines exactly.

**Branch model:** `develop` is the integration branch where all work lands;
`main` is release-only and changes exclusively via a release PR from
`develop`. The two workflows split along that line.

## `ci.yml` — Quality gates (everything except release)

Runs the fast, pure-logic checks that keep `develop` green. Triggered on any
push to `develop` and on every pull request; running jobs are cancelled when a
newer push supersedes them.

| Job | Purpose |
|---|---|
| `fmt` | `cargo fmt --all -- --check` — enforces the shared formatting. |
| `clippy` | `cargo clippy --all-targets -- -D warnings` — lints all targets, warnings are errors. |
| `test` | `cargo test --all-targets` plus `cargo test --doc` — unit, integration, and doc tests. |
| `msrv` | `cargo check --all-targets` on Rust 1.85 — proves the published MSRV (`rust-version` in `Cargo.toml`) still compiles. |
| `audit` | `rustsec/audit-check@v2` — blocks on known vulnerabilities in the dependency tree. |
| `deny` | `embarkStudios/cargo-deny-action@v2` — enforces the license allowlist and dependency policy in `deny.toml`. |

> A `test-integration` job (like the adapter's — live Keycloak + Postgres via
> the repo's `docker-compose.yml` stack, `cargo test --test integration -- --ignored`)
> will be added once the CLI gains its integration-test suite during the crate
> scaffold.

## `release.yml` — Releases (PRs to `main` and their merges)

Automates the entire publish flow with [release-plz], driven by Conventional
Commits. Triggered only by activity on `main`: a `pull_request` targeting
`main` (the release PR) and the `push` that merge produces. It needs
`contents: write` and `pull-requests: write` because it manages branches,
opens PRs, creates tags, and publishes.

| Job | Purpose |
|---|---|
| `gate` | (runs on the PR to `main`) Quality gate: fmt, clippy, tests, and `cargo package`. Nothing ships unless this passes. |
| `release-plz` | (runs on the merge into `main`) `release-plz/action@v0.5` with `command: release-pr`. Opens/updates the release PR (version bump + `CHANGELOG.md`); once merged it creates the `keystate-cli-v<version>` tag, publishes to crates.io, and creates the GitHub release. Requires the `CARGO_REGISTRY_TOKEN` secret. |

> The release-plz job opens PRs with the built-in `GITHUB_TOKEN`. If the org
> blocks that, enable **"Allow GitHub Actions to create and approve pull
> requests"** under Settings → Actions → General → Workflow permissions, or
> pass a fine-grained PAT as `GITHUB_TOKEN` (see `RELEASE.md` §2).

## Secrets used

- `CARGO_REGISTRY_TOKEN` — crates.io token with publish rights (`release.yml`).
- `GITHUB_TOKEN` — built-in, used by both workflows (audit + release-plz).

See `RELEASE.md` for the full release process and one-time setup.

[release-plz]: https://release-plz.enyx.fr/