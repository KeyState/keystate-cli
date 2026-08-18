//! Configuration-file model and resolution.
//!
//! One concern per module:
//!
//! - this module — the on-disk TOML data model (`Config` and its sections);
//! - [`file`] — loading and parsing the file;
//! - [`resolve`] — the precedence chain that turns args + config + environment
//!   into the effective options for one run.

mod file;
pub mod resolve;

use serde::Deserialize;

pub use file::load;

/// The on-disk TOML configuration model.
///
/// Every field is optional; the resolver combines these with defaults,
/// environment variables, and command-line flags.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// The `[backend]` section.
    #[serde(default)]
    pub backend: BackendSection,
    /// The `[extract]` section.
    #[serde(default)]
    pub extract: ExtractSection,
    /// The `[output]` section.
    #[serde(default)]
    pub output: OutputSection,
}

/// Backend selection.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendSection {
    /// Backend name, e.g. `keycloak`.
    #[serde(default)]
    pub name: Option<String>,
}

/// Extraction parameters.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractSection {
    /// Realm to extract.
    #[serde(default)]
    pub realm: Option<String>,
    /// Backend database URL. May reference an environment variable as
    /// `${NAME}`. Prefer that over an embedded secret.
    #[serde(default)]
    pub db_url: Option<String>,
}

/// Output destination.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputSection {
    /// Output root directory.
    #[serde(default)]
    pub directory: Option<String>,
}
