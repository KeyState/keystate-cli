//! The durable local-file sink: atomic, idempotent, history-preserving.
//!
//! Writes each extraction under a fresh timestamped directory and points the
//! `latest` pointer file at it. Every file goes through a sibling temp file
//! and an atomic rename, so an interrupted process can never leave a truncated
//! artifact where `--check` or a restore would read it. `--check` needs no
//! write path at all: it recomputes the artifacts and compares them against
//! the latest stored `config.json`.

use std::fs;
use std::path::Path;

use keystate_core::{CanonicalRealm, VerificationReport, canonical_bytes, config_bytes};

use super::layout::{CONFIG_FILE, Layout, REPORT_FILE, utc_timestamp};
use crate::error::Result;

/// The outcome of comparing a computed extraction against the last one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    /// No previous extraction exists at the destination.
    New,
    /// The computed output is byte-identical to the last extraction.
    Unchanged,
    /// The computed output differs from the last extraction (drift).
    WouldChange,
}

impl CheckStatus {
    /// The stable, scriptable status string used in output.
    pub fn as_str(self) -> &'static str {
        match self {
            CheckStatus::New => "new",
            CheckStatus::Unchanged => "unchanged",
            CheckStatus::WouldChange => "would-change",
        }
    }
}

/// What one write produced.
#[derive(Debug, Clone)]
pub struct Written {
    /// The timestamped run directory.
    pub run_dir: std::path::PathBuf,
    /// `config.json` path.
    pub config_path: std::path::PathBuf,
    /// `report.json` path.
    pub report_path: std::path::PathBuf,
}

/// Persists extractions under `<root>/<ts>/` and maintains the `latest` pointer.
pub struct LocalFileSink {
    layout: Layout,
}

impl LocalFileSink {
    /// Build a sink, validating the realm as a safe path component up front
    /// (so an invalid realm fails before any database connection is made).
    pub fn new(output_root: &Path, backend: &str, realm: &str) -> Result<Self> {
        Ok(Self {
            layout: Layout::new(output_root, backend, realm)?,
        })
    }

    /// The layout in use.
    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// Serialize one extraction into its two artifacts: the canonical config
    /// and the completeness report.
    pub fn artifacts(
        &self,
        canonical: &CanonicalRealm,
        report: &VerificationReport,
    ) -> Result<(Vec<u8>, Vec<u8>)> {
        Ok((config_bytes(canonical)?, canonical_bytes(report)?))
    }

    /// The bytes of the most recent extraction's `config.json`, if any.
    pub fn latest_config(&self) -> Result<Option<Vec<u8>>> {
        match self.layout.latest_run_dir()? {
            Some(dir) => match fs::read(dir.join(CONFIG_FILE)) {
                Ok(bytes) => Ok(Some(bytes)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error.into()),
            },
            None => Ok(None),
        }
    }

    /// Compare a computed config against the last one without writing.
    pub fn check(&self, computed_config: &[u8]) -> Result<CheckStatus> {
        match self.latest_config()? {
            None => Ok(CheckStatus::New),
            Some(previous) if previous == computed_config => Ok(CheckStatus::Unchanged),
            Some(_) => Ok(CheckStatus::WouldChange),
        }
    }

    /// Write one extraction atomically under a fresh timestamped directory and
    /// point `latest` at it.
    pub fn write(
        &self,
        canonical: &CanonicalRealm,
        report: &VerificationReport,
    ) -> Result<Written> {
        self.write_at(canonical, report, chrono::Utc::now())
    }

    /// [`write`](Self::write) with an explicit clock, for deterministic tests.
    pub fn write_at(
        &self,
        canonical: &CanonicalRealm,
        report: &VerificationReport,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Written> {
        let (config, report_bytes) = self.artifacts(canonical, report)?;
        let timestamp = utc_timestamp(now);
        let run_dir = self.layout.run_dir(&timestamp);
        fs::create_dir_all(&run_dir)?;

        let config_path = run_dir.join(CONFIG_FILE);
        let report_path = run_dir.join(REPORT_FILE);
        atomic_write(&config_path, &config)?;
        atomic_write(&report_path, &report_bytes)?;
        atomic_write(&self.layout.latest_file(), timestamp.as_bytes())?;

        Ok(Written {
            run_dir,
            config_path,
            report_path,
        })
    }
}

/// Write `bytes` to `path` via a sibling temp file + rename.
///
/// `rename` is atomic within a directory, so a reader either sees the old
/// file or the complete new one — never a half-written artifact.
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use keystate_core::{BackendInfo, CanonicalRealm, Version};

    fn realm() -> CanonicalRealm {
        CanonicalRealm {
            backend: BackendInfo {
                backend: "keycloak".into(),
                detected_version: Version::new(26, 7, 0),
                extra: Default::default(),
            },
            ..CanonicalRealm::default()
        }
    }

    fn report() -> VerificationReport {
        VerificationReport {
            backend: "keycloak".into(),
            against_version: Version::new(26, 7, 0),
            issues: Vec::new(),
        }
    }

    #[test]
    fn write_creates_timestamped_artifacts_and_latest_pointer() {
        let dir = tempfile::tempdir().unwrap();
        let sink = LocalFileSink::new(dir.path(), "keycloak", "master").unwrap();
        let written = sink.write(&realm(), &report()).unwrap();

        assert!(written.config_path.exists());
        assert!(written.report_path.exists());
        assert_eq!(written.config_path.file_name().unwrap(), "config.json");
        assert_eq!(written.report_path.file_name().unwrap(), "report.json");

        let latest = sink.layout().latest_file();
        let name = std::fs::read_to_string(&latest).unwrap();
        assert_eq!(
            written.run_dir,
            dir.path().join("keycloak/master").join(name.trim())
        );
    }

    #[test]
    fn write_is_history_preserving_and_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let sink = LocalFileSink::new(dir.path(), "keycloak", "master").unwrap();

        let now = chrono::DateTime::parse_from_rfc3339("2026-08-16T14:30:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let first = sink.write_at(&realm(), &report(), now).unwrap();
        let second = sink
            .write_at(&realm(), &report(), now + chrono::Duration::seconds(1))
            .unwrap();

        // Two distinct run directories, byte-identical artifacts.
        assert_ne!(first.run_dir, second.run_dir);
        assert_eq!(
            std::fs::read(first.config_path).unwrap(),
            std::fs::read(second.config_path).unwrap()
        );

        // latest now points at the second run.
        let name = std::fs::read_to_string(sink.layout().latest_file()).unwrap();
        assert_eq!(second.run_dir, sink.layout().root().join(name.trim()));
    }

    #[test]
    fn artifacts_are_sorted_and_volatile_free() {
        let dir = tempfile::tempdir().unwrap();
        let sink = LocalFileSink::new(dir.path(), "keycloak", "master").unwrap();
        let (config, _report) = sink.artifacts(&realm(), &report()).unwrap();
        let text = String::from_utf8(config).unwrap();
        assert!(
            !text.contains("\"volatile\""),
            "volatile leaked into config"
        );
        let a = text.find("\"backend\"").unwrap();
        let z = text.find("\"clients\"").unwrap();
        assert!(a < z, "object keys must be sorted: {text}");
    }

    #[test]
    fn check_reports_new_unchanged_would_change() {
        let dir = tempfile::tempdir().unwrap();
        let sink = LocalFileSink::new(dir.path(), "keycloak", "master").unwrap();
        let (config, _report) = sink.artifacts(&realm(), &report()).unwrap();

        // No baseline yet.
        assert_eq!(sink.check(&config).unwrap(), CheckStatus::New);

        // Baseline written.
        sink.write(&realm(), &report()).unwrap();
        assert_eq!(sink.check(&config).unwrap(), CheckStatus::Unchanged);

        // Introduce drift in the stored artifact.
        let latest = sink.layout().latest_run_dir().unwrap().unwrap();
        let config_path = latest.join(CONFIG_FILE);
        fs::write(&config_path, b"{\"mutated\":true}").unwrap();
        assert_eq!(sink.check(&config).unwrap(), CheckStatus::WouldChange);
    }

    #[test]
    fn atomic_write_leaves_no_temp_file_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        atomic_write(&path, b"{\"a\":1}").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"{\"a\":1}");
        assert!(!dir.path().join("config.tmp").exists());
    }
}
