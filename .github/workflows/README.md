# GitHub Actions — Workflows

This directory contains the CI/CD pipeline for `keystate-cli`: `ci.yml`
(quality gates) and `release.yml` (GHCR + GitHub Releases publishing). Both
run on the same GNU/Linux runner (`ubuntu-latest`) with the stable Rust
toolchain and share the caching from `Swatinem/rust-cache`. The CI side
mirrors the `keystate-core` and `keystate-adapter-keycloak` pipelines.

**Branch model:** `develop` is the integration branch where all work lands;
`main` is release-only and changes exclusively via a release PR from
`develop`. `keystate-cli` is **not** published to crates.io — it ships as a
binary and a container image on GHCR (see `RELEASE.md`), so there is no
crates.io publishing pipeline here. Every PR — regardless of base branch,
`develop` or `main` — runs the full `ci.yml` suite, which is what gates the
release PR to `main`.

## `ci.yml` — Quality gates

Runs the fast, pure-logic checks that keep `develop` green and gate the
release PR. Triggered on any push to `develop` and on every pull request;
running jobs are cancelled when a newer push supersedes them.

| Job | Purpose |
|---|---|
| `fmt` | `cargo fmt --all -- --check` — enforces the shared formatting. |
| `clippy` | `cargo clippy --all-targets -- -D warnings` — lints all targets, warnings are errors. |
| `test` | `cargo test --all-targets` plus `cargo test --doc` — unit, integration, and doc tests. |
| `test-integration` | Runs the repo's `docker-compose.yml` stack (Keycloak 26.5 + Postgres) on the runner via `docker compose up -d --wait`, then runs the live-DB suite (`tests/integration.rs`, the `#[ignore]`d tests) that drives the real `keystate` binary and verifies its output — realm-export.json (importable: no ids), report.json, byte-identical re-extraction, and `--check` drift semantics (exit 3). |
| `import-test` | The round-trip gate: starts the stack and imports **every** file in `contrib/example-config/` (the committed golden files) with keycloak-config-cli, so anything that once imported keeps importing. A code change that breaks the round-trip fails here. See `RELEASE.md` §2. |
| `msrv` | `cargo check --all-targets` on Rust 1.85 — proves the published MSRV (`rust-version` in `Cargo.toml`) still compiles. |
| `audit` | `rustsec/audit-check@v2` — blocks on known vulnerabilities in the dependency tree. |
| `deny` | `embarkStudios/cargo-deny-action@v2` — enforces the license allowlist and dependency policy in `deny.toml`. |
| `features` | `cargo tree -e features` must not contain `preserve_order` — Cargo unifies features per build, so a transitive crate enabling it would silently flip serde_json's map backing and break canonical byte stability. This job fails the build if it appears anywhere in the tree. |

## `release.yml` — GHCR + GitHub Releases

Triggered only by a tag push (`v*`). Builds the release binary, pushes the
container image to **GHCR** (`ghcr.io/keystate/keystate-cli`, `latest` +
version), and creates the GitHub Release with the binary tarball and
checksum. GHCR needs nothing to configure — the built-in `GITHUB_TOKEN` with
`packages: write` does the auth; there is no Docker Hub publishing.

| Step | Notes |
|---|---|
| `Gate` | fmt, clippy `-D warnings`, unit + doc tests — nothing ships on a red gate. |
| Build + archive | `cargo build --release --locked`, tarball + `sha256`. |
| Log in to GHCR | `GITHUB_TOKEN` + `packages: write` — no secret to manage. |
| Build + push image | `docker/build-push-action@v6`, `cache-from/to: type=gha`. |
| GitHub Release | `softprops/action-gh-release@v2`, release notes auto-generated. |

## Secrets used

- `GITHUB_TOKEN` — built-in (audit/deny, GHCR login, GitHub Release).

See `RELEASE.md` for the full release process and one-time setup.