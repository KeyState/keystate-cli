//! Run progress and machine-readable status output.
//!
//! Two reporting modes, one module:
//!
//! - **Text** (default): human-readable progress lines on stderr — always on
//!   stderr so stdout stays clean for the machine-readable status. Suppressed
//!   entirely by `--quiet`.
//! - **JSON**: a single status object on stdout, nothing else, so the tool
//!   can be wired into CI/alerting without scraping human-formatted text.

use clap::ValueEnum;
use keystate_core::{BackendInfo, VerificationReport};
use serde_json::json;

/// How the CLI reports progress and results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable progress lines on stderr.
    Text,
    /// A single JSON status line on stdout.
    Json,
}

/// Emits progress to stderr and, at the end, an optional status object.
pub struct Progress {
    quiet: bool,
    format: OutputFormat,
}

/// A one-line machine-readable summary of a completed run.
#[derive(Debug, Clone, Copy)]
pub struct Status<'a> {
    /// `"ok"` or `"drift"`.
    pub status: &'a str,
    /// For `--check` runs: `new`, `unchanged`, or `would-change`.
    pub check: Option<&'a str>,
    /// Backend identifier.
    pub backend: &'a str,
    /// Realm extracted.
    pub realm: &'a str,
    /// Detected backend version.
    pub version: &'a str,
    /// Whether verification found no `Error`-severity findings.
    pub complete: bool,
    /// Number of `Error`-severity verification findings.
    pub errors: usize,
    /// Number of warning-severity verification findings.
    pub warnings: usize,
    /// The run directory written (absent for `--check` runs).
    pub output: Option<&'a std::path::Path>,
}

impl Progress {
    /// Build a reporter from the effective run options.
    pub fn new(quiet: bool, format: OutputFormat) -> Self {
        Self { quiet, format }
    }

    /// A step line, emitted only in text mode when not quiet.
    pub fn step(&self, message: impl AsRef<str>) {
        if !self.quiet && self.format == OutputFormat::Text {
            eprintln!("{}", message.as_ref());
        }
    }

    /// Report backend detection.
    pub fn detecting(&self, info: &BackendInfo) {
        self.step(format!(
            "Detecting backend... {} {}",
            info.backend, info.detected_version
        ));
    }

    /// Report the start of extraction for a realm.
    pub fn extracting(&self, realm: &str) {
        self.step(format!("Extracting realm '{realm}'..."));
    }

    /// Report verification against the version's manifest.
    pub fn verifying(&self, report: &VerificationReport) {
        let (errors, warnings) = report.counts();
        self.step(format!(
            "Verifying completeness: {} errors, {} warnings",
            errors, warnings
        ));
    }

    /// Report the start of realm-export rendering.
    pub fn rendering(&self) {
        self.step("Rendering realm-export...");
    }

    /// Report the destination of a write.
    pub fn writing(&self, path: &std::path::Path) {
        self.step(format!("Writing output to {}", path.display()));
    }

    /// Report the outcome of a `--check` run.
    pub fn checked(&self, status: &str) {
        self.step(format!("Check: {status}"));
    }

    /// Emit a machine-readable status object on stdout in JSON mode.
    ///
    /// A single line, nothing else: `{"status":"ok","backend":...,...}`.
    pub fn emit_status(&self, status: &Status<'_>) {
        if self.format != OutputFormat::Json {
            return;
        }
        let object = json!({
            "status": status.status,
            "check": status.check,
            "backend": status.backend,
            "realm": status.realm,
            "version": status.version,
            "complete": status.complete,
            "errors": status.errors,
            "warnings": status.warnings,
            "output": status.output.map(|p| p.to_string_lossy()),
        });
        println!("{object}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_text_mode_emits_nothing() {
        let progress = Progress::new(true, OutputFormat::Text);
        progress.step("hidden");
        progress.extracting("master");
        progress.verifying(&VerificationReport {
            backend: "stub".into(),
            against_version: keystate_core::Version::new(1, 0, 0),
            issues: Vec::new(),
        });
    }
}
