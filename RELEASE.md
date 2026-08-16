# Keystate — Release Process & Upstream Tracking

This document covers two related but distinct concerns: how Keystate itself
gets released across five repos without drifting out of sync, and how the
project stays current with upstream Keycloak and FerrisKey schema changes
without discovering them from a user's bug report.

---

## 1. Versioning Policy

Every repo follows semver independently, but the *meaning* of a version bump
differs by repo:

| Repo | A "major" bump means | A "minor" bump means | A "patch" bump means |
|---|---|---|---|
| `keystate-core` | A canonical model field/struct removed or changed incompatibly | A new field/struct added, additively | Bug fix, no model change |
| `keystate-adapter-*` | Drops support for a previously-supported backend version, or a breaking core dependency bump | Adds support for a new backend version, or a new field extraction | Bug fix, no behavior change |
| `keystate-cli` | Breaking CLI flag/output-format change | New feature, new adapter bundled | Bug fix, dependency bump only |

`keystate-core` is the contract. Its version is the one everything else is
pinned against, so changes there get the most scrutiny — a major bump
there forces a major bump in every adapter that hasn't absorbed the change
yet, which is expensive across five repos. Prefer additive changes whenever
the model allows it.

**Promotions are a special release case (and a release-train obligation).** A
promotion (same native field seen in two or more backends, moved into common,
per the architecture document §3.1) is an additive minor bump in `keystate-core`
and always gets a changelog entry naming the fields and the backends that
justified the move. But an adapter that has not absorbed the new common field
extracts it missing, and the verifier then flags healthy realms as incomplete
— version skew misread as drift. So a promotion is only tagged once *all* live
adapters ship compatible minor bumps, and `keystate-cli` only tags a combination
after every constituent repo has absorbed it. A promotion is never released
piecemeal across the org.

## 2. Release Mechanics

Releases are automated end to end with [release-plz], driven by the
Conventional Commits in the repository history. The pipeline lives in
`.github/workflows/release.yml`; CI gates live in `.github/workflows/ci.yml`.

**Branches: `develop` integrates, `main` releases.** All work merges into
`develop` via PR and is gated by `ci.yml`. Nothing is ever pushed to `main`
directly. A release is the deliberate act of opening a **release PR from
`develop` to `main`** and merging it.

- **A release PR to `main` is gated, then merged.** `release.yml` runs a
  quality gate (fmt, clippy `-D warnings`, tests, `cargo package`) on the PR
  to `main`. Once merged, release-plz computes what changed since the last
  release:
  - if there are unreleased commits, it opens/updates a **release PR** that
    bumps `Cargo.toml`, appends to `CHANGELOG.md`, and labels the PR;
  - once *that* PR is merged, it **creates the git tag
    (`keystate-cli-v<version>`), publishes the crate to crates.io, and
    creates the GitHub release** — all in CI, from the `release-pr` command.
- **No direct pushes to `main`, no manual tagging, no manual `cargo publish`.**
  The only way `main` changes is a merged PR from `develop`; the crates.io
  token exists only as the `CARGO_REGISTRY_TOKEN` repository secret.
- **Releases only happen on green gates.** `release.yml` gates the merge PR
  before it lands; `ci.yml` runs fmt, clippy, the unit suite, doctests, an
  MSRV check (1.85), `cargo audit`, and `cargo-deny` on every PR and push to
  `develop`.
- **Semver is derived, not decided by hand.** Commit types map to the bump
  (`feat:` → minor, `fix:`/`chore:` → patch, breaking → major), which keeps
  the versioning policy above mechanical rather than a judgement call per
  release.
- **First publish is the one manual step.** The crate name must be reserved
  on crates.io and a publish-capable token stored as the
  `CARGO_REGISTRY_TOKEN` secret before the first automated release. See
  "First release: one-time setup" below.

### Publish targets per repo

- `keystate-core` and each adapter publish as crates (crates.io once the
  project is public and stable enough to commit to that namespace; a private
  registry is fine in the meantime).
- `keystate-cli` is the only repo that builds and publishes the actual
  distributable: the binary release on GitHub Releases, and the Docker
  image, pushed on every tagged release via CI.

### First release: one-time setup

1. **Reserve the name.** Run `cargo publish` once manually (or add the user
   to the crates.io crate) so `keystate-cli` belongs to an account you
   control. Until then the automated publish has nothing to publish to.
2. **Add the token.** Under repo Settings → Secrets → Actions, create
   `CARGO_REGISTRY_TOKEN` with a crates.io token that has publish rights.
3. **Allow Actions to open the release PR.** release-plz opens the PR with
   the built-in `GITHUB_TOKEN`. If the org blocks that, the run fails with
   `GitHub Actions is not permitted to create or approve pull requests`.
   Fix under repo Settings → Actions → General → *Workflow permissions*:
   check **Allow GitHub Actions to create and approve pull requests**. If the
   org forbids enabling it, use a fine-grained PAT (read/write on contents
   and pull requests) stored as a secret and pass it as `GITHUB_TOKEN`
   instead — the workflow's `permissions` block then needs no `contents`/PR
   grant.
4. Merge a `feat:` (or `fix:`) PR to `develop`. When ready, open the release
   PR from `develop` to `main` and merge it. The pipeline does the rest:
   release PR → merge → tag → crates.io publish → GitHub release.

### No release goes out on a failing build

The scheduled backend-matrix and completeness-regression tests (from the
development document) act as a further gate — if a nightly run against
`develop` is red, the fix lands before the next release, not after.

### Branch protection

Protect `develop` (require PRs + review) so no work bypasses `ci.yml`, and
protect `main` (require PRs, require status checks, disallow force push and
direct pushes) so the only path into `main` is a reviewed release PR. This is
what makes the "no pushes to main" rule hold mechanically rather than by
convention.

[release-plz]: https://release-plz.enyx.fr/

## 3. The Compatibility Matrix

Because Keystate's whole value proposition rests on completeness, "which
version of Keystate supports which version of Keycloak/FerrisKey" needs to
be a documented, tested fact — not a claim. This lives in `keystate-docs` as
a generated table, not a hand-maintained one:

| keystate-cli | keystate-core | adapter-keycloak | Keycloak versions tested | adapter-ferriskey | FerrisKey versions tested |
|---|---|---|---|---|---|
| 1.4.0 | 1.2.0 | 1.3.0 | 24.x – 26.x | 0.2.0 | main @ 2026-06 |

Each row is populated automatically from CI's integration test matrix results
at release time — if a version combination wasn't actually tested green in
CI, it doesn't appear in the matrix as supported. This is also where you'd
publish a deprecation timeline when dropping support for an old backend
version, so users have a documented window rather than a surprise gap.

## 4. Tracking Upstream: Keycloak

Keycloak's schema changes are visible before they hit a release, via its
Liquibase changelogs in the main Keycloak repository. The tracking process:

1. **Automated schema-diff check.** A scheduled CI job in
   `keystate-adapter-keycloak` pulls the latest Keycloak Docker image
   (tracking both the latest stable and the current RC/milestone if one
   exists), runs the adapter's schema introspection against it, and diffs
   the result against the last known schema fingerprint stored in the repo.
2. **On a detected diff, the job opens an issue automatically** — labeled
   `upstream-schema-change` — summarizing what changed (new table, new
   column, altered constraint). This turns "did something change upstream"
   from a manual research task into a notification that shows up on its own.
3. **A human triages the issue**: does this map to something the canonical
   model should represent? If yes, it becomes a `feat:` following the
   standard core → adapter → cli flow from the development document. If it's
   internal to Keycloak and irrelevant to configuration state (e.g. a purely
   operational/cache table), it gets closed with a note explaining why, so
   the reasoning is preserved for the next person who wonders the same
   thing.
4. **Release notes and major Keycloak version announcements** are worth a
   lighter-weight manual skim in addition to the automated diff — schema
   changes are only part of what matters; deprecations or behavioral changes
   to fields Keystate already extracts (e.g. a field being repurposed rather
   than added) won't necessarily show up as a schema diff but could still
   silently change what the extracted data *means*.

## 5. Tracking Upstream: FerrisKey

Same underlying goal, adjusted for a much younger, faster-moving project:

- FerrisKey's schema changes land as normal commits/migrations in its own
  repository rather than a versioned changelog process as mature as
  Keycloak's Liquibase history. The equivalent automated check here watches
  FerrisKey's migrations directory directly (via a scheduled job comparing
  against the last-seen commit hash) rather than diffing a running
  instance's schema, since stable release tags may be sparser early on.
- Given the project's early stage, expect this to fire more often and with
  less predictability than the Keycloak check. Budget triage time for it
  accordingly rather than treating an infrequent Keycloak-style cadence as
  the default assumption.
- Because FerrisKey adoption is still small, it's reasonable — and
  worthwhile — to engage directly with FerrisKey's own maintainers about
  schema stability plans, rather than purely reacting to diffs after the
  fact. A young project's maintainers are often glad to hear from a
  downstream consumer of their schema.

## 6. Deprecation Policy

When a backend version is dropped from support (typically once it's past its
own upstream end-of-life):

- Announce in the adapter's changelog at least one minor release ahead of
  actually dropping it.
- Update the compatibility matrix with an explicit deprecation date, not just
  silent removal.
- Keep the last adapter version that supported it easily discoverable (a
  pinned note in the README) for anyone who needs to stay on it temporarily.