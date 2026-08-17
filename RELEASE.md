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

**Branches: `develop` integrates, `main` releases.** All work merges into
`develop` via PR and is gated by `ci.yml`. Nothing is ever pushed to `main`
directly. A release is the deliberate act of opening a **release PR from
`develop` to `main`** and merging it.

- **A release PR to `main` is gated, then merged.** `ci.yml` runs on every
  pull request — including one targeting `main` — so the release PR gets the
  full gate (fmt, clippy `-D warnings`, unit + integration + doc tests, live
  Keycloak integration, MSRV 1.85, `cargo audit`, `cargo deny`, the
  `preserve_order` guard) before it can land.
- **`keystate-cli` is not published to crates.io.** It ships as a binary on
  GitHub Releases and a container image on GHCR, so there is no crates.io
  publish step, no `CARGO_REGISTRY_TOKEN`, and no release-plz pipeline.
  Consequently the CLI depends on `keystate-core` and
  `keystate-adapter-keycloak` directly via git — a permanent arrangement, not
  a pre-release workaround. Keep those pins on stable branches (`develop`),
  never feature branches, once the adapter's extraction work merges.
- **A release is: merge the release PR, then tag.** After the release PR to
  `main` merges, push the tag `keystate-cli-v<version>`. The tag triggers
  `.github/workflows/release.yml`, which gates (fmt/clippy/tests), builds the
  release binary, pushes the image to **GHCR**
  (`ghcr.io/keystate/keystate-cli`, `latest` + version), and creates the
  **GitHub Release** with the binary tarball and `sha256` checksum. No other
  registry — Docker Hub is out of scope for now.
- **No direct pushes to `main`, no manual `cargo publish`.** The only way
  `main` changes is a merged PR from `develop`.
- **Releases only happen on green gates.** `ci.yml` gates every PR and push
  to `develop`; the release PR to `main` runs the same suite; and
  `release.yml` re-runs fmt/clippy/tests on the tag before publishing
  anything.
- **Versions are bumped in the release PR.** Bump `Cargo.toml` in the release
  PR itself. The org follows the semver policy in §1; Conventional Commit
  types in the merged history tell you what the bump should be.

### GHCR image — one-time setup

- **Nothing to configure.** The tag workflow logs in with the built-in
  `GITHUB_TOKEN` (`packages: write`). Packages default to private on GitHub —
  flip `ghcr.io/keystate/keystate-cli` to public in Package settings if you
  want anonymous pulls.

### Publish targets per repo

- `keystate-core` and each adapter publish as crates (crates.io once the
  project is public and stable enough to commit to that namespace; a private
  registry is fine in the meantime).
- `keystate-cli` is the only repo that builds and publishes the actual
  distributable — the binary release on GitHub Releases and the GHCR image —
  and it is **not** published to crates.io.

### Branch protection

Protect `develop` (require PRs + review) so no work bypasses `ci.yml`, and
protect `main` (require PRs, require status checks, disallow force push and
direct pushes) so the only path into `main` is a reviewed release PR. This is
what makes the "no pushes to main" rule hold mechanically rather than by
convention.

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