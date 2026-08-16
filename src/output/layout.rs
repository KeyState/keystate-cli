//! Output path and filename rules.
//!
//! The layout preserves history while keeping "where is the current state"
//! trivial:
//!
//! ```text
//! <output>/<backend>/<realm>/<utc-timestamp>/config.json
//!                                   /report.json
//!                    /latest          <- names the most recent run directory
//! ```
//!
//! Every extraction writes a fresh, timestamped directory, so nothing is ever
//! overwritten in place (the DR/retention story: "restore to Tuesday 3am").
//! The `latest` pointer file is what `--check` and casual users read.
//!
//! Realm names become filesystem path components and are validated here, so a
//! realm such as `../x` cannot write outside the intended output tree.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Name of the canonical config artifact.
pub const CONFIG_FILE: &str = "config.json";
/// Name of the completeness report artifact.
pub const REPORT_FILE: &str = "report.json";
/// Name of the pointer file that names the most recent run directory.
pub const LATEST_FILE: &str = "latest";

/// Validates and holds the output paths for one backend/realm pair.
#[derive(Debug, Clone)]
pub struct Layout {
    /// `<output>/<backend>/<realm>`.
    root: PathBuf,
}

impl Layout {
    /// Build a layout, validating the realm as a safe path component.
    pub fn new(output_root: &Path, backend: &str, realm: &str) -> Result<Self> {
        let realm_component = sanitize_realm_component(realm)?;
        Ok(Self {
            root: output_root.join(backend).join(realm_component),
        })
    }

    /// `<output>/<backend>/<realm>`.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `<root>/<ts>` — a fresh directory per extraction run.
    pub fn run_dir(&self, timestamp: &str) -> PathBuf {
        self.root.join(timestamp)
    }

    /// The `latest` pointer file path.
    pub fn latest_file(&self) -> PathBuf {
        self.root.join(LATEST_FILE)
    }

    /// Resolve the most recent run directory, if one exists.
    pub fn latest_run_dir(&self) -> Result<Option<PathBuf>> {
        match std::fs::read_to_string(self.latest_file()) {
            Ok(name) => Ok(Some(self.root.join(name.trim()))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

/// A UTC timestamp safe to use as a directory name, sortable
/// lexicographically, and unique at sub-second precision so two runs within
/// the same second cannot collide.
pub fn utc_timestamp(now: chrono::DateTime<chrono::Utc>) -> String {
    now.format("%Y%m%dT%H%M%S%.6fZ").to_string()
}

/// A realm name must be safe to use as a single filesystem path component.
///
/// Rejects anything that could escape the output tree (`../`, absolute
/// paths, embedded separators) or otherwise produce a surprising path.
pub fn sanitize_realm_component(realm: &str) -> Result<String> {
    if realm.is_empty() {
        return Err(Error::InvalidRealm {
            realm: realm.to_string(),
            reason: "realm name is empty",
        });
    }
    if realm == "." || realm == ".." {
        return Err(Error::InvalidRealm {
            realm: realm.to_string(),
            reason: "must not be a relative path component",
        });
    }
    if realm.contains('/') || realm.contains('\\') || realm.contains('\0') {
        return Err(Error::InvalidRealm {
            realm: realm.to_string(),
            reason: "must be a single path component without separators",
        });
    }
    if realm.chars().any(char::is_control) {
        return Err(Error::InvalidRealm {
            realm: realm.to_string(),
            reason: "must not contain control characters",
        });
    }
    Ok(realm.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_is_sortable_utc() {
        let ts = utc_timestamp(
            chrono::DateTime::parse_from_rfc3339("2026-08-16T14:30:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        );
        assert_eq!(ts, "20260816T143000.000000Z");
    }

    #[test]
    fn valid_realm_names_pass() {
        assert_eq!(sanitize_realm_component("master").unwrap(), "master");
        assert_eq!(
            sanitize_realm_component("my-realm_2").unwrap(),
            "my-realm_2"
        );
    }

    #[test]
    fn traversal_and_separators_are_rejected() {
        for realm in ["../evil", "..", ".", "a/b", "a\\b", "a\0b"] {
            let error = sanitize_realm_component(realm).unwrap_err();
            assert!(
                matches!(error, Error::InvalidRealm { .. }),
                "expected InvalidRealm for {realm:?}, got {error}"
            );
        }
    }

    #[test]
    fn empty_realm_is_rejected() {
        let error = sanitize_realm_component("").unwrap_err();
        assert!(matches!(error, Error::InvalidRealm { .. }), "{error}");
    }

    #[test]
    fn layout_points_latest_at_the_named_run() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path(), "keycloak", "master").unwrap();
        let run = layout.run_dir("20260816T143000Z");
        std::fs::create_dir_all(&run).unwrap();
        std::fs::write(layout.latest_file(), "20260816T143000Z\n").unwrap();
        assert_eq!(
            layout.latest_run_dir().unwrap().unwrap(),
            dir.path().join("keycloak/master/20260816T143000Z")
        );
    }

    #[test]
    fn layout_without_latest_yields_none() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path(), "keycloak", "master").unwrap();
        assert!(layout.latest_run_dir().unwrap().is_none());
    }
}
