//! The `extract` subcommand's argument surface.
//!
//! Purely declarative: names, help text, and types. The *resolution* of
//! these values against config files and the environment lives in
//! [`crate::config::resolve`].

use std::path::PathBuf;

use clap::Args;

use crate::progress::OutputFormat;

/// Arguments for `keystate extract`.
#[derive(Debug, Args)]
pub struct ExtractArgs {
    /// Path to a TOML configuration file. Flags override values from it.
    #[arg(short, long, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Backend to extract from. Defaults to `keycloak`.
    #[arg(short, long, value_name = "NAME")]
    pub backend: Option<String>,

    /// Realm to extract.
    #[arg(short, long, value_name = "NAME")]
    pub realm: Option<String>,

    /// Backend database URL. Discouraged: prefer KEYSTATE_DB_URL or the
    /// interactive prompt (see `--help`).
    #[arg(short = 'd', long, value_name = "URL")]
    pub db_url: Option<String>,

    /// Output root directory. Defaults to `./keystate-out`.
    #[arg(short, long, value_name = "DIR")]
    pub output: Option<PathBuf>,

    /// Run the full pipeline without writing, and report drift against the
    /// last extraction (exit code 3 when the output would change).
    #[arg(long)]
    pub check: bool,

    /// Suppress human-readable progress output.
    #[arg(short, long)]
    pub quiet: bool,

    /// Emit a single machine-readable JSON status line on stdout.
    #[arg(long, value_enum, value_name = "FORMAT", default_value_t = OutputFormat::Text)]
    pub output_format: OutputFormat,
}
