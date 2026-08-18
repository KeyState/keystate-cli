# Keystate — Development & Testing Workflow

This document describes how work moves through the Keystate repos, day to
day. It applies to every repo in the organization unless a repo's own README
says otherwise.

## 1. Branching Model

Two protected branches, and everything between them flows through pull
requests:

- **`develop` — integration branch, where all work lands.** Feature and fix
  branches are created off `develop`, and merged back into `develop` via PR
  (at least one review required). Short-lived: created off develop, merged
  back in, deleted. All quality gates in `ci.yml` run here — on every push to
  `develop` and on every PR.
- **`main` — release-only.** Nothing is pushed to `main` directly and feature
  work never merges there. The *only* way `main` changes is a release PR from
  `develop`, which is exactly what's merged when a release is wanted. The PR
  to `main` runs the same `ci.yml` gate as any other PR; once merged, the
  maintainer tags it (see `RELEASE.md`).

Releases are a deliberate act, not an ambient side effect of merging work:

- a PR targets `main` only when it's meant to ship;
- `develop` keeps accumulating ordinary work without ever touching `main`;
- this mirrors the five-repo flow in the release document, where adapters and
  the CLI pin tagged core versions rather than publishing every core change.

Branch naming: `feat/<short-description>`, `fix/<short-description>`,
`chore/<short-description>`. Keep it short enough to read in a PR list.

**Commit convention:** [Conventional Commits](https://www.conventionalcommits.org/)
(`feat:`, `fix:`, `chore:`, `test:`, `docs:`). This isn't bureaucracy for its
own sake — it's what drives automated changelog generation and semver bumps
in the release process (see the companion release document), so it needs to
be consistent from the first commit.

## 2. The Standard Feature Flow

Because the domain model lives in `keystate-core` and adapters depend on it,
most non-trivial features touch more than one repo, in a fixed order. Take
"add support for extracting client scope mappers" as a worked example:

### Step 1 — Does this change the canonical model?

Ask this first, always — but expect the answer to be "no" most of the time.
Since canonical structs carry only a thin set of common fields plus an open
`native: serde_json::Value`, the large majority of feature work is "extract
this field Keycloak already has and put it in native, and add its path to
the manifest" — entirely within the adapter repo, no core release required.
That's intentional: it's what keeps adapter velocity decoupled from core's
release cadence.

A core change is only needed for one of these:

- A field is a genuine promotion candidate — it now appears in native for two
  or more backends and belongs in common for cross-backend comparison to
  work. Promotions are additive minor bumps in core, and get a changelog
  entry naming the fields and backends that justified the move. They also
  carry a release-train obligation: all live adapters must absorb the new
  common field before any CLI tags a combination including it, or
  verification will read healthy realms as "incomplete" due to version skew
  (see `RELEASE.md` §1).
- A new envelope-level concept, not an entity-level one — e.g. the envelope
  needs to carry something new about `backend_info` itself.
- A new cross-cutting mechanism — e.g. the volatile flag or the
  canonical-writer contract being introduced or changed, which every adapter
  needs to honor.

If none of those apply, skip straight to Step 3 and stay entirely in the
adapter repo.

### Step 2 — `keystate-core`

- Branch off `develop`.
- Extend the canonical model (e.g. add a `ClientScopeMapper` struct, or a
  field on an existing struct). Keep new fields additive where possible —
  adding a field is a minor version bump; changing or removing one is a
  major bump and breaks every adapter that hasn't been updated yet.
- Update the contract test suite so any `Extractor` implementation is now
  required to populate the new field, if it's meant to be mandatory.
- Update the `FieldManifest` / completeness-verification data so the new
  field is something the verifier actually checks for, not just a struct
  sitting unused.
- Open a PR to `develop`. Once a group of changes is ready to ship, open the
  release PR from `develop` to `main`; merging it publishes per the release
  document. **Nothing downstream should start consuming the new core version
  until it's actually tagged and published** — don't build against a branch.

### Step 3 — Adapter (e.g. `keystate-adapter-keycloak`)

- Bump the `keystate-core` dependency in `Cargo.toml` to the new tagged
  version.
- Implement the actual extraction logic: the query against Keycloak's
  schema, the mapping into the new canonical struct.
- Add an integration test against a real Keycloak instance (via
  `testcontainers`, see Section 4) that proves the new field is actually
  populated correctly, not just that the code compiles.
- Add a determinism test if the new data introduces any new ordering
  concerns (see Section 4.4).
- Open a PR to `develop`. Releases happen via the release PR from `develop`
  to `main`.

### Step 4 — `keystate-cli`

- Bump both `keystate-core` and the adapter dependency to their new tagged
  versions.
- Run the full local test suite and the compatibility matrix check.
- Update the CLI's own compatibility matrix entry if this changes which
  backend versions are supported.
- Open a PR to `develop`. When the combination is ready, merge the release
  PR to `main` — this is the artifact that actually ships to users, so it's
  the one release that triggers a Docker image build and publish.

The rule of thumb: **core → adapter → cli, always in that order, never
skipped.** An adapter should never depend on an unreleased core commit, and
the CLI should never depend on an unreleased adapter commit. This keeps each
repo's history independently buildable and independently auditable — which
matters for a tool people are trusting with database credentials.

### Step 5 — Adapter-only features

Most day-to-day work will actually just be Step 3 and 4 — most features are
"extract this field Keycloak already has that we haven't mapped yet," not
new canonical concepts. Same discipline applies, just starting one level in.

## 3. Local Development Setup

Since the repos are genuinely separate (not a Cargo workspace), local
cross-repo iteration uses Cargo's `[patch]` mechanism rather than publishing
a new core version for every experiment:

```toml
# In keystate-cli/Cargo.toml, while developing locally
[patch."https://github.com/keystate/keystate-core"]
keystate-core = { path = "../keystate-core" }
[patch."https://github.com/keystate/keystate-adapter-keycloak"]
keystate-adapter-keycloak = { path = "../keystate-adapter-keycloak" }
```

Remove the patch before opening a PR — CI should always build against
published versions, not local paths, so what's tested is what will actually
be released.

For exercising adapters against a real backend, each adapter repo should
ship a `docker-compose.yml` that spins up the target backend (Keycloak +
Postgres, or FerrisKey + Postgres) pre-seeded with a representative realm —
this is what both local development and CI integration tests run against.

## 4. Testing Strategy

Four layers, each catching a different class of problem:

### 4.1 Unit tests (every repo)

Standard `cargo test` coverage of pure logic — canonical model
serialization, mapping functions, the completeness-verification engine
itself. No database involved. Run on every commit, every PR, fast.

### 4.2 Contract tests (`keystate-core`, run by every adapter)

`keystate-core` ships a reusable test suite that any `Extractor`
implementation must pass — this is what actually enforces the port contract
beyond the type signature. Every adapter repo imports and runs this suite
against its own implementation as part of CI.

The suite covers: canonical serialization determinism (serialize twice,
assert byte-identical, roundtrip), volatile segregation (every
manifest-volatile path lands in the volatile section and nowhere else),
manifest self-consistency, and synthetic-case verifier checks (empty realm,
zero-row entities, missing required fields, misplaced volatile fields). See
`ARCHITECTURE.md` §7.

### 4.3 Integration tests (adapter repos)

Real backend, real database, via `testcontainers`. These run against a
matrix of supported backend versions (e.g. the last 3–4 Keycloak minor
versions) so a schema change in a specific version is caught in CI, not
reported by a user. These are slower — run on every PR to the adapter repo,
and on a schedule (nightly) against `main` to catch drift from upstream
backend releases even when no PR is open (see the release document's
upstream-tracking section).

### 4.4 Determinism / idempotency tests (adapter and CLI repos)

Run extraction twice against an unchanged fixture database, hash both
outputs, assert the hashes match. This is a direct, automated check on the
idempotency property from the architecture document — any PR that breaks it
should fail CI, not get caught in manual review.

### 4.5 Completeness regression tests (adapter repos, scheduled)

Periodically extract from a throwaway Keycloak instance via Keystate, and
separately via Keycloak's own official CLI export (the stop-server one),
then diff the two. This is the strongest available check that the DB-based
extraction isn't silently missing something the official tooling captures —
run it on a schedule rather than every PR, since it requires stopping a
test server.

## 5. What CI Runs, and When

CI lives in `.github/workflows/ci.yml` (quality gates) and
`.github/workflows/release.yml` (tag-triggered publishing). `keystate-cli`
is not published to crates.io — releases are git tags on `main` that push
the GHCR image and a GitHub Release (see `RELEASE.md`).

| Trigger | Checks |
|---|---|
| Every PR (to develop or main) and every push to develop | `cargo fmt --check`, `cargo clippy -- -D warnings`, unit + doc tests, MSRV check (1.85), `cargo audit`, `cargo deny`, `cargo tree -e features` must not contain `preserve_order` |
| PR to main (the release PR) | The full `ci.yml` suite above — since `pull_request` in `ci.yml` is unfiltered, a PR to `main` is gated exactly like any other |
| Release tag push (`v*`) | `release.yml`: re-gates fmt/clippy/tests, builds the release binary, pushes `ghcr.io/keystate/keystate-cli` (`latest` + version), creates the GitHub Release with the binary + checksum |
| Nightly, scheduled | Full backend version matrix (integration tests across all supported versions), completeness regression test (adapter repos) |
| Integration (live Keycloak) | Runs the repo's `docker-compose.yml` stack and the `tests/integration.rs` suite (`cargo test --test integration -- --ignored`) — drives the real binary and verifies the output: realm-export.json / report.json content (importable shape: no ids), byte-identical re-extraction, and `--check` drift semantics (exit 3) |

The integration suite needs the compose stack; on a PR it runs in the
`test-integration` job, locally via `docker compose up -d --wait` and
`cargo test --test integration -- --ignored`. The `preserve_order` guard is
the second line of defense behind core's explicit key sorting: Cargo unifies
features per build, so a transitive crate enabling serde_json's
`preserve_order` would silently break canonical byte stability.

`keystate-cli` is never published to crates.io, but it depends on the
**released** `keystate-core` (0.1.0) and `keystate-adapter-keycloak` (0.3.0)
from crates.io — the release-train contract: once an adapter releases, the
CLI follows to the released versions, because mixing a git core with the
adapter's registry core would be two different crates and the types would not
unify. Pointing the deps at git `develop` branches is a temporary measure for
the pre-release phase only; a released crate carries no git dependencies.

## 6. Code Review Checklist

Beyond the usual correctness review, reviewers on Keystate PRs specifically
check:

- **Does this preserve determinism?** Any new collection must be sorted by a
  stable key before serialization; any new map must use `BTreeMap`, not
  `HashMap`, if key order could otherwise vary.
- **Is this canonical-model change additive or breaking?** Breaking changes
  need a clear justification and a major version bump — they're not free.
- **Is the completeness manifest updated alongside any new field?** A field
  that exists in the model but isn't tracked by the verifier is a silent gap
  in exactly the property (completeness) this tool exists to guarantee.
- **Does any new output path keep the trust boundary intact?** Data never
  transits Keystate-operated infrastructure; a new sink is a new
  `OutputSink` implementation running in the user's own environment, never a
  Keystate-owned endpoint (ARCHITECTURE.md §3.4).
- **Are secrets or credential data ever logged, printed, or included in
  error messages?** Given what this tool touches, this is a hard no in every
  review, not a style preference.