# keystate-cli

The Keystate command-line tool — composes `keystate-core` and the backend
adapters into a distributable binary. It extracts an IAM realm's
configuration state directly out of the backend's own database, verifies it
against the version's completeness manifest, and writes an idempotent,
history-preserving snapshot you can diff, audit, and restore from.

> **Status:** `keystate extract` (Keycloak) is the first milestone. A second
> backend and a `--check`-based drift pipeline are the roadmap's next steps.

## Install / build

```sh
cargo build --release
```

Requires Rust 1.85+ (the published MSRV).

## Quick start

Extract the `master` realm from a running Keycloak:

```sh
# Preferred: credentials from the environment, never the shell history.
export KEYSTATE_DB_URL=postgres://keycloak:keycloak@localhost:5432/keycloak
keystate extract --backend keycloak --realm master
```

Or everything on the command line:

```sh
keystate extract --backend keycloak --realm master \
  --db-url postgres://keycloak:keycloak@localhost:5432/keycloak
```

Or a configuration file (flags override the file):

```sh
keystate extract --config keystate.toml
```

## Usage

```
Usage: keystate <COMMAND>

Commands:
  extract  Extract one realm's configuration state from a backend and write it out
```

### `keystate extract`

| Flag | Meaning |
|---|---|
| `-c, --config <FILE>` | TOML config file. Flags override its values. |
| `-b, --backend <NAME>` | Backend to extract from (default `keycloak`). |
| `-r, --realm <NAME>` | Realm to extract. |
| `-d, --db-url <URL>` | Backend database URL. **Discouraged** — prefer `KEYSTATE_DB_URL` or the prompt. |
| `-o, --output <DIR>` | Output root (default `./keystate-out`). |
| `--check` | Run the pipeline but write nothing; report drift vs the last extraction (exit `3` when the output would change). |
| `-q, --quiet` | Suppress progress lines. |
| `--output-format <FORMAT>` | `text` (default) or `json` (one machine-readable status line on stdout). |

#### Credentials precedence

`config file < KEYSTATE_DB_URL < --db-url < interactive prompt`

- `KEYSTATE_DB_URL` is the recommended path — it never appears in `ps`,
  shell history, or the repository.
- The prompt only fires when nothing else is set **and** stdin is a terminal,
  so cron/CI fails fast instead of hanging.
- A config file can reference the environment without embedding a secret:
  `db_url = "${DATABASE_URL}"`.
- Using `--db-url` or a plaintext `db_url` prints a one-line warning.

#### Output layout

```text
<output>/<backend>/<realm>/<utc-timestamp>/realm-export.json  # importable realm-export (Keycloak format, no ids)
                                     /report.json   # completeness report
                     /latest          # names the most recent run directory
```

Every run writes a fresh timestamped directory — nothing is overwritten in
place — and the `latest` pointer names the current state. Re-running against
an unchanged source is byte-identical (the idempotency guarantee), so a diff
between snapshots reflects real configuration drift, never extraction noise.

`realm-export.json` is the tool's config artifact: Keycloak's realm-export
format, emitted without `id` fields or id-references so a tool like
keycloak-config-cli can import it back (the round-trip the project validates
before every release — see `RELEASE.md`).

#### Drift detection

```sh
keystate extract --config keystate.toml --check --output-format json
```

Reports `new` / `unchanged` / `would-change` and exits `3` on drift. That is
the mechanism a scheduled cron uses to alert when a realm's configuration has
changed:

```sh
if ! keystate extract --config keystate.toml --check --quiet; then
  # exit 3: drift detected — notify your compliance/audit channel
fi
```

Exit codes: `0` success, `1` runtime error, `2` usage error, `3` drift.

## Config file reference

```toml
[backend]
name = "keycloak"

[extract]
realm = "master"
db_url = "${DATABASE_URL}"   # reference an env var; never embed the secret

[output]
directory = "./keystate-out"
```

## Run with the container image

Release images are published to GHCR (`ghcr.io/keystate/keystate-cli`,
tagged with the version and `latest`):

```sh
docker pull ghcr.io/keystate/keystate-cli:latest
docker run --rm \
  -e KEYSTATE_DB_URL=postgres://keycloak:keycloak@localhost:5432/keycloak \
  -v "$PWD/keystate-out:/out" \
  ghcr.io/keystate/keystate-cli:latest extract --realm master --output /out
```

The image runs as a non-root user; mount an output directory the container
can write to.

## Development

- `docker compose up -d --wait` — start the local Keycloak + Postgres stack.
- `cargo test` — unit + doc tests.
- `cargo test --test integration -- --ignored` — end-to-end tests against the
  live stack (see `DEVELOPMENT.md` for the full workflow).

## License

Apache-2.0