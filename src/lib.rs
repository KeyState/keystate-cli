//! # keystate-cli
//!
//! The Keystate command-line tool — composes `keystate-core` and the backend
//! adapters into a distributable binary. This crate owns the *pipeline*:
//! argument parsing, config/environment resolution, backend dispatch, output
//! layout, and the idempotent, history-preserving write. Each concern lives
//! in its own module; this file only wires them together.
//!
//! See `README.md` for usage and `DEVELOPMENT.md` / `RELEASE.md` for the
//! workflow, or run `keystate --help`.

#![warn(missing_docs)]

pub mod backend;
pub mod cli;
pub mod config;
pub mod error;
pub mod output;
pub mod progress;
pub mod redact;

use clap::Parser;

pub use cli::{Cli, Command};
pub use error::{Error, Result};

use output::CheckStatus;
use progress::Status;

/// The terminal outcome of a run; `main` maps it to a process exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// Success. Exit code 0.
    Ok,
    /// `--check` found that the output would change (drift). Exit code 3.
    Drift,
}

/// Run one CLI invocation to completion.
pub async fn run(cli: Cli) -> Result<Exit> {
    match cli.command {
        Command::Extract(args) => {
            let creds = config::resolve::System;
            let cfg = match &args.config {
                Some(path) => Some(config::load(path)?),
                None => None,
            };
            let opts = config::resolve::resolve(&args, cfg.as_ref(), &creds)?;
            let progress = progress::Progress::new(opts.quiet, opts.output_format);

            // Validate the realm and output path *before* touching the
            // database, so a bad realm fails fast and offline.
            let sink =
                output::LocalFileSink::new(&opts.output_dir, opts.backend.name(), &opts.realm)?;

            let (canonical, report, export) =
                backend::extract(opts.backend, &opts.realm, &opts.db_url, &progress).await?;

            let (errors, warnings) = report.counts();
            let version = canonical.backend.detected_version.to_string();
            let complete = report.is_complete();

            if opts.check {
                let (export_bytes, _report) = sink.artifacts(&export, &report)?;
                let status = sink.check(&export_bytes)?;
                progress.checked(status.as_str());
                progress.emit_status(&Status {
                    status: status_string(status),
                    check: Some(status.as_str()),
                    backend: opts.backend.name(),
                    realm: &opts.realm,
                    version: &version,
                    complete,
                    errors,
                    warnings,
                    output: None,
                });
                return Ok(match status {
                    CheckStatus::Unchanged | CheckStatus::New => Exit::Ok,
                    CheckStatus::WouldChange => Exit::Drift,
                });
            }

            let written = sink.write(&export, &report)?;
            progress.writing(&written.run_dir);
            progress.emit_status(&Status {
                status: "ok",
                check: None,
                backend: opts.backend.name(),
                realm: &opts.realm,
                version: &version,
                complete,
                errors,
                warnings,
                output: Some(&written.run_dir),
            });
            Ok(Exit::Ok)
        }
    }
}

/// The run-level status string used in machine-readable output.
fn status_string(status: CheckStatus) -> &'static str {
    match status {
        CheckStatus::Unchanged | CheckStatus::New => "ok",
        CheckStatus::WouldChange => "drift",
    }
}

/// Convenience for `main`: parse argv and run.
pub fn main_argv() -> Result<Exit> {
    let cli = Cli::parse();
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|source| Error::Message(format!("failed to start async runtime: {source}")))?;
    runtime.block_on(run(cli))
}
